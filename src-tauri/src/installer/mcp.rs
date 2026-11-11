use std::path::{Path, PathBuf};

use serde_json::Value;

use super::cleanup::{DirectoryGuard, StagingAttempt};
use super::mcp_contract::{PACKAGE_JSON, PACKAGE_LOCK, validate_contract};
use super::mcp_fs::{private_directory, validate_installation, write_new};
use super::npm::NpmEnvironment;
use super::paths::InstallerPaths;

/// A validated staged installation. No external MCP client configuration is
/// read or changed. Call activate to consume it; dropping it retains staging
/// because a cancelled subprocess may not yet have been reaped.
pub struct StagedMcp {
    paths: InstallerPaths,
    staged: PathBuf,
    attempt: Option<StagingAttempt>,
    runtime_guard: DirectoryGuard,
    lock: Value,
}

impl StagedMcp {
    pub fn path(&self) -> &Path {
        &self.staged
    }

    /// Abandon completed staging without deletion. The durable receipt and all
    /// staging resources remain for manual review; this releases the live lease.
    pub async fn discard_async(self) -> Result<(), String> {
        let deadline = self
            .attempt
            .as_ref()
            .ok_or("Missing attempt")?
            .deadline
            .clone();
        super::deadline::run_with(deadline, move || async move { self.discard() }).await
    }

    pub fn discard(mut self) -> Result<(), String> {
        self.attempt
            .take()
            .ok_or("Missing staging ownership")?
            .cleanup()
            .map(|_| ())
            .map_err(|_| "MCP staging cleanup refused or failed".into())
    }

    /// Use from async callers so validation and sealing stay off their executor.
    pub async fn activate_async(self) -> Result<(), String> {
        let deadline = self
            .attempt
            .as_ref()
            .ok_or("Missing attempt")?
            .deadline
            .clone();
        super::deadline::run_with(deadline, move || async move { self.activate() }).await
    }

    pub fn activate(mut self) -> Result<(), String> {
        let deadline = self
            .attempt
            .as_ref()
            .ok_or("Missing attempt")?
            .deadline
            .clone();
        let _scope = deadline.enter();
        deadline.check()?;
        let activation = self
            .runtime_guard
            .verify()
            .map_err(|_| "Managed runtime changed during installation".to_string())
            .and_then(|()| {
                self.attempt
                    .as_ref()
                    .ok_or("Missing staging ownership")?
                    .verify()
                    .map_err(|_| "Staging ownership changed".to_string())
            })
            .and_then(|()| {
                let attempt = self.attempt.as_ref().ok_or("Missing attempt ownership")?;
                super::limits::MCP.check(attempt)?;
                attempt.verify_checkpoint()?;
                attempt.state(super::attempt::Lifecycle::Activating)?;
                super::progress::milestone(super::api_models::Stage::Activating);
                activate_mcp(&self.paths, &self.staged, &self.lock)?;
                attempt
                    .checkpoint(super::attempt::Lifecycle::Activated)
                    .map_err(|error| {
                        format!("MCP activated but durable checkpoint failed: {error}")
                    })
            });
        self.finish(activation)
    }

    fn finish<T>(&mut self, result: Result<T, String>) -> Result<T, String> {
        let cleanup = self
            .attempt
            .take()
            .ok_or("Missing staging ownership")?
            .cleanup();
        match (result, cleanup) {
            (Ok(value), Ok(_)) => Ok(value),
            (Err(error), Ok(_)) => Err(format!(
                "{error}; staging retained for manual review; automatic cleanup disabled"
            )),
            (Ok(_), Err(_)) => Err("MCP activated, but staging cleanup refused or failed".into()),
            (Err(error), Err(_)) => Err(format!("{error}; staging cleanup refused or failed")),
        }
    }
}

impl Drop for StagedMcp {
    fn drop(&mut self) {
        if self.attempt.is_some() {
            // Do not race a process being terminated by a cancelled future.
            // No paths, environment values, or subprocess output are logged.
            eprintln!(
                "MCP installation interrupted or abandoned; private staging retained for recovery"
            );
        }
    }
}

pub async fn install_mcp(paths: &InstallerPaths) -> Result<(), String> {
    let paths = paths.clone();
    super::deadline::run(move || async move { stage_mcp_inner(&paths).await?.activate() }).await
}

pub async fn stage_mcp(paths: &InstallerPaths) -> Result<StagedMcp, String> {
    let paths = paths.clone();
    super::deadline::run(move || async move { stage_mcp_inner(&paths).await }).await
}

async fn stage_mcp_inner(paths: &InstallerPaths) -> Result<StagedMcp, String> {
    super::progress::milestone(super::api_models::Stage::Verifying);
    let lock = validate_contract(PACKAGE_JSON, PACKAGE_LOCK)?;
    if !cfg!(target_os = "macos") {
        return Err("MCP installation currently requires macOS".into());
    }
    if !paths.root.is_absolute() {
        return Err("Installer root must be absolute".into());
    }
    if paths.root.components().any(|part| {
        matches!(
            part,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    }) || paths.runtime != paths.root.join("runtime")
        || paths.mcp != paths.root.join("mcp")
        || paths.staging != paths.root.join("staging")
    {
        return Err("Installer paths must remain inside the managed root".into());
    }
    ensure_destination_absent(&paths.mcp)?;
    super::root::prepare_installation_root(&paths.root)
        .map_err(|_| "Cannot prepare installation root")?;
    let runtime_guard = DirectoryGuard::capture(&[&paths.root, &paths.runtime])
        .map_err(|_| "Managed Node.js runtime is unavailable")?;
    let attempt = StagingAttempt::create(paths, super::attempt::InstallationKind::Mcp)?;
    let mut staged = StagedMcp {
        staged: attempt.path().join("mcp"),
        paths: paths.clone(),
        attempt: Some(attempt),
        runtime_guard,
        lock,
    };
    let attempt = staged.attempt.as_ref().ok_or("Missing attempt ownership")?;
    attempt.state(super::attempt::Lifecycle::Running)?;
    let result = super::limits::monitor(attempt, super::limits::MCP, populate(&staged))
        .await
        .and_then(|()| attempt.checkpoint(super::attempt::Lifecycle::Prepared));
    match result {
        Ok(()) => Ok(staged),
        Err(error) => staged.finish(Err(error)),
    }
}

async fn populate(staged: &StagedMcp) -> Result<(), String> {
    let attempt = staged.attempt.as_ref().ok_or("Missing staging ownership")?;
    private_directory(&staged.staged)?;
    write_new(&staged.staged.join("package.json"), PACKAGE_JSON.as_bytes())?;
    write_new(
        &staged.staged.join("package-lock.json"),
        PACKAGE_LOCK.as_bytes(),
    )?;
    let npm = NpmEnvironment::create(attempt.path(), &staged.paths.runtime, &staged.staged)?;
    super::progress::milestone(super::api_models::Stage::Validating);
    super::validate::validate_node_runtime(&staged.paths.runtime).await?;
    verify_ownership(staged)?;
    super::progress::milestone(super::api_models::Stage::Installing);
    npm.ci().await?;
    super::progress::milestone(super::api_models::Stage::Validating);
    verify_ownership(staged)?;
    validate_installation(&staged.staged, &staged.lock)?;
    npm.validate_dependencies().await?;
    verify_ownership(staged)?;
    validate_installation(&staged.staged, &staged.lock)
}

fn verify_ownership(staged: &StagedMcp) -> Result<(), String> {
    staged
        .runtime_guard
        .verify()
        .map_err(|_| "Managed runtime identity changed")?;
    staged
        .attempt
        .as_ref()
        .ok_or("Missing staging ownership")?
        .verify()
        .map_err(|_| "Staging directory identity changed".into())
}

fn ensure_destination_absent(destination: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(destination) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err("An MCP installation already exists".into()),
        Err(_) => Err("Cannot inspect MCP destination".into()),
    }
}

pub(super) fn activate_mcp(
    paths: &InstallerPaths,
    source: &Path,
    lock: &Value,
) -> Result<(), String> {
    let parent = source.parent().ok_or("Invalid MCP staging path")?;
    if source.file_name() != Some(std::ffi::OsStr::new("mcp"))
        || parent.parent() != Some(paths.staging.as_path())
    {
        return Err("MCP must be activated from Figcon staging".into());
    }
    let guard = DirectoryGuard::capture(&[&paths.root, &paths.staging, parent, source])
        .map_err(|_| "MCP activation requires real managed directories")?;
    ensure_destination_absent(&paths.mcp)?;
    validate_installation(source, lock)?;
    guard.verify().map_err(|_| "MCP staging identity changed")?;
    super::deadline::check()?;
    super::activate::rename_without_replacement(source, &paths.mcp)
        .map_err(|_| "Atomic MCP activation failed; existing destination preserved".into())
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
pub(super) mod tests;
