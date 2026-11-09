use std::fs;

use super::manifest::MCP_VERSION;
use super::models::{InstallationState, InstallationStatus};
use super::paths::InstallerPaths;
use super::resolve_installer_paths;

#[tauri::command]
pub fn get_installation_status(app: tauri::AppHandle) -> Result<InstallationStatus, String> {
    Ok(inspect_installation(&resolve_installer_paths(&app)?))
}

fn status(state: InstallationState, message: &'static str) -> InstallationStatus {
    InstallationStatus {
        installed_version: (state == InstallationState::Installed).then(|| MCP_VERSION.to_owned()),
        state,
        target_version: MCP_VERSION.to_owned(),
        message: Some(message.to_owned()),
    }
}

/// Read-only structural status shared by the Tauri command and integration
/// tests. No network, process execution, config discovery, or repair occurs.
/// This is not a fresh cryptographic attestation of installed file contents.
pub fn inspect_installation(paths: &InstallerPaths) -> InstallationStatus {
    match inspect(paths) {
        Ok(true) => status(
            InstallationState::Installed,
            "Managed installation passed structural validation.",
        ),
        Ok(false) => status(
            InstallationState::NotInstalled,
            "MCP is not installed in this managed root.",
        ),
        Err(()) => status(
            InstallationState::Error,
            "Managed installation is incomplete, unsafe, or unreadable.",
        ),
    }
}

fn present(path: &std::path::Path) -> Result<bool, ()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(()),
    }
}

fn inspect(paths: &InstallerPaths) -> Result<bool, ()> {
    if !paths.root.is_absolute()
        || paths.runtime != paths.root.join("runtime")
        || paths.mcp != paths.root.join("mcp")
        || paths.staging != paths.root.join("staging")
    {
        return Err(());
    }
    if !present(&paths.root)? {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(&paths.root).map_err(|_| ())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o777 != 0o700
        {
            return Err(());
        }
    }
    let runtime = present(&paths.runtime)?;
    let mcp = present(&paths.mcp)?;
    if !runtime {
        return if mcp { Err(()) } else { Ok(false) };
    }
    let runtime_guard =
        super::cleanup::DirectoryGuard::capture(&[&paths.root, &paths.runtime]).map_err(|_| ())?;
    super::validate::inspect_node_runtime(&paths.runtime).map_err(|_| ())?;
    if !mcp {
        runtime_guard.verify().map_err(|_| ())?;
        return Ok(false);
    }
    let guard = super::cleanup::DirectoryGuard::capture(&[&paths.root, &paths.runtime, &paths.mcp])
        .map_err(|_| ())?;
    let lock = super::mcp_contract::validate_contract(
        super::mcp_contract::PACKAGE_JSON,
        super::mcp_contract::PACKAGE_LOCK,
    )
    .map_err(|_| ())?;
    super::mcp_fs::validate_installation(&paths.mcp, &lock).map_err(|_| ())?;
    runtime_guard.verify().map_err(|_| ())?;
    guard.verify().map_err(|_| ())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_status_is_read_only_and_incomplete_mcp_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon-installer"));
        assert_eq!(
            inspect_installation(&paths).state,
            InstallationState::NotInstalled
        );
        assert!(!paths.root.exists());
        fs::create_dir(&paths.root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&paths.root, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::create_dir(&paths.mcp).unwrap();
        assert_eq!(inspect_installation(&paths).state, InstallationState::Error);
    }

    #[cfg(unix)]
    #[test]
    fn damaged_runtime_and_symlink_root_are_errors_without_repair() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallerPaths::new(temp.path().join("figcon-installer"));
        super::super::root::prepare_installation_root(&paths.root).unwrap();
        super::super::test_support::write_runtime(&paths.runtime);
        assert_eq!(
            inspect_installation(&paths).state,
            InstallationState::NotInstalled
        );
        fs::remove_file(paths.runtime.join(super::super::npm::NPM_CLI)).unwrap();
        assert_eq!(inspect_installation(&paths).state, InstallationState::Error);
        assert!(!paths.runtime.join(super::super::npm::NPM_CLI).exists());
        let linked = InstallerPaths::new(temp.path().join("linked"));
        std::os::unix::fs::symlink(&paths.root, &linked.root).unwrap();
        assert_eq!(
            inspect_installation(&linked).state,
            InstallationState::Error
        );
    }
}
