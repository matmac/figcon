//! Trusted path resolution and the real production installers, never IPC paths.
use super::{api_models::*, paths::InstallerPaths};
pub(super) fn supported() -> bool {
    cfg!(all(
        target_os = "macos",
        any(target_arch = "aarch64", target_arch = "x86_64")
    ))
}

pub(super) async fn execute(paths: InstallerPaths, component: Component) -> Result<(), String> {
    if !supported() {
        return Err("Unsupported platform".into());
    }
    #[cfg(target_os = "macos")]
    let _operation_lock = operation_lock(&paths)?;
    let destination = match component {
        Component::Node => &paths.runtime,
        Component::Mcp => &paths.mcp,
    };
    match std::fs::symlink_metadata(destination) {
        Ok(_) => return Err("API destination exists".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Cannot inspect destination".into()),
    }
    if component == Component::Mcp {
        super::validate::inspect_node_runtime(&paths.runtime)
            .map_err(|_| "API runtime required")?;
    }
    match component {
        Component::Node => {
            super::install::install_node_runtime(
                &paths,
                if cfg!(target_arch = "aarch64") {
                    "arm64"
                } else {
                    "x64"
                },
            )
            .await
        }
        Component::Mcp => super::mcp::install_mcp(&paths).await,
    }
}

#[cfg(target_os = "macos")]
pub(super) fn operation_lock(paths: &InstallerPaths) -> Result<std::fs::File, String> {
    use std::os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    };
    super::root::prepare_installation_root(&paths.root)
        .map_err(|_| "Cannot prepare managed root")?;
    // Serialize creation/validation with all cooperating staging allocators.
    let _admission = super::admission::lock(paths)?;
    let guard = super::cleanup::DirectoryGuard::capture(&[&paths.root, &paths.metadata])
        .map_err(|_| "Unsafe operation root")?;
    let path = paths.metadata.join("operation.lock");
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|_| "Cannot open operation lock")?;
    let m = file
        .metadata()
        .map_err(|_| "Cannot inspect operation lock")?;
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o777 != 0o600
    {
        return Err("Unsafe operation lock".into());
    }
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("API operation busy".into());
    }
    guard.verify().map_err(|_| "Operation root changed")?;
    let current = std::fs::symlink_metadata(path).map_err(|_| "Operation lock missing")?;
    if current.dev() != m.dev() || current.ino() != m.ino() {
        return Err("Operation lock replaced".into());
    }
    Ok(file)
}

fn inspect_root(paths: &InstallerPaths) -> Result<(), ()> {
    match std::fs::symlink_metadata(&paths.root) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(()),
        Ok(m) => {
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err(());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o777 != 0o700 {
                    return Err(());
                }
            }
            Ok(())
        }
    }
}
fn component(paths: &InstallerPaths, component: Component) -> ComponentReport {
    let destination = match component {
        Component::Node => &paths.runtime,
        Component::Mcp => &paths.mcp,
    };
    let inspected =
        inspect_root(paths).and_then(|()| match std::fs::symlink_metadata(destination) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(()),
            Ok(_) => {
                let valid = match component {
                    Component::Node => {
                        super::cleanup::DirectoryGuard::capture(&[&paths.root, &paths.runtime])
                            .map_err(|_| ())
                            .and_then(|g| {
                                super::validate::inspect_node_runtime(&paths.runtime)
                                    .map_err(|_| ())?;
                                g.verify().map_err(|_| ())
                            })
                    }
                    Component::Mcp => {
                        if super::status::inspect_installation(paths).state
                            == super::models::InstallationState::Installed
                        {
                            Ok(())
                        } else {
                            Err(())
                        }
                    }
                };
                valid.map(|()| true)
            }
        });
    ComponentReport {
        disk_state: match inspected {
            Ok(true) => DiskState::Installed,
            Ok(false) => DiskState::NotInstalled,
            Err(()) => DiskState::Invalid,
        },
        component,
        state: match inspected {
            Ok(true) => OperationState::Installed,
            Ok(false) => OperationState::NotInstalled,
            Err(()) => OperationState::Failed,
        },
        target_version: component.version().into(),
        installed_version: matches!(inspected, Ok(true)).then(|| component.version().into()),
        last_operation: None,
        error: inspected
            .err()
            .map(|_| InstallationError::new(ErrorCode::ValidationFailed)),
    }
}
pub(super) fn inspect(paths: &InstallerPaths) -> ManagedInstallationReport {
    let retention = if supported() {
        match super::recovery::recover_installations(paths) {
            Ok(r) => RetentionReport {
                state: if r.retained.is_empty() && r.orphaned_ledger_files == 0 {
                    RetentionState::None
                } else {
                    RetentionState::RequiresReview
                },
                attempt_count: Some(r.retained.len() as u32),
                orphaned_ledger_files: Some(r.orphaned_ledger_files as u32),
            },
            Err(_) => RetentionReport {
                state: RetentionState::Unknown,
                attempt_count: None,
                orphaned_ledger_files: None,
            },
        }
    } else {
        RetentionReport {
            state: RetentionState::Unknown,
            attempt_count: None,
            orphaned_ledger_files: None,
        }
    };
    ManagedInstallationReport {
        revision: 0,
        supported: supported(),
        node: component(paths, Component::Node),
        mcp: component(paths, Component::Mcp),
        retention,
    }
}
