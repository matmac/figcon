//! Native qualification selection. No IPC parameters and no release override reader.
use std::path::PathBuf;
use tauri::Manager;

#[derive(Clone)]
pub(crate) struct Roots {
    pub home: PathBuf,
    pub data: PathBuf,
    pub executables: PathBuf,
    #[cfg(all(debug_assertions, target_os = "macos"))]
    anchor: macos::Anchor,
}
pub(crate) struct Session {
    roots: Option<Roots>,
}
impl Session {
    pub(crate) fn from_process() -> Result<Self, &'static str> {
        #[cfg(debug_assertions)]
        {
            let enabled = std::env::var_os("FIGCON_NATIVE_ACCEPTANCE");
            let root = std::env::var_os("FIGCON_NATIVE_ACCEPTANCE_ROOT");
            let executable =
                std::env::current_exe().map_err(|_| "Cannot identify native executable")?;
            return Self::select_process(enabled.as_deref(), root.as_deref(), &executable);
        }
        #[cfg(not(debug_assertions))]
        Ok(Self { roots: None }) // Release never reads acceptance environment variables.
    }
    #[cfg(debug_assertions)]
    fn select_process(
        enabled: Option<&std::ffi::OsStr>,
        root: Option<&std::ffi::OsStr>,
        executable: &std::path::Path,
    ) -> Result<Self, &'static str> {
        // A dedicated test copy must not silently downgrade to production paths
        // when LaunchServices/automation loses its environment. This name check
        // only denies startup; it never grants ownership or selects a root.
        #[cfg(target_os = "macos")]
        if enabled.is_none()
            && root.is_none()
            && executable.ancestors().any(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_prefix("figcon-native-"))
                    .is_some_and(|suffix| {
                        suffix.len() == 32 && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
                    })
            })
        {
            return Err("Acceptance executable requires explicit opt-in");
        }
        #[cfg(not(target_os = "macos"))]
        let _ = executable;
        Self::select(enabled, root)
    }
    #[cfg(debug_assertions)]
    fn select(
        enabled: Option<&std::ffi::OsStr>,
        root: Option<&std::ffi::OsStr>,
    ) -> Result<Self, &'static str> {
        if enabled.is_none() && root.is_none() {
            return Ok(Self { roots: None });
        }
        if enabled != Some(std::ffi::OsStr::new("1")) {
            return Err("Invalid native acceptance selection");
        }
        #[cfg(target_os = "macos")]
        {
            return Ok(Self {
                roots: Some(macos::load(std::path::Path::new(
                    root.ok_or("Missing native acceptance root")?,
                ))?),
            });
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = root;
            Err("Native acceptance is supported only on macOS")
        }
    }
    pub(crate) fn roots(&self) -> Result<Option<Roots>, &'static str> {
        #[cfg(all(debug_assertions, target_os = "macos"))]
        if let Some(roots) = &self.roots {
            roots.anchor.verify()?;
        }
        Ok(self.roots.clone())
    }
    pub(crate) fn configure(
        &self,
        config: &mut tauri::utils::config::Config,
    ) -> Result<(), &'static str> {
        if let Some(roots) = self.roots()? {
            config.app.app_directories_override = Some(
                tauri::utils::config::AppDirectoriesOverride::Root(roots.data),
            );
            for window in &mut config.app.windows {
                window.incognito = true;
            }
        }
        Ok(())
    }
}
pub(crate) fn roots(app: &tauri::AppHandle) -> Result<Option<Roots>, &'static str> {
    app.try_state::<Session>()
        .ok_or("Discovery environment unavailable")?
        .roots()
}
// Legacy commands must not inspect any host clients in isolated mode, including
// when the selected root was replaced and validation now fails.
pub(crate) fn legacy_blocked(app: &tauri::AppHandle) -> bool {
    !matches!(roots(app), Ok(None))
}
#[cfg(all(debug_assertions, target_os = "macos"))]
mod macos {
    use super::*;
    use std::{
        ffi::CString,
        fs::File,
        io::Read,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::{Component, Path},
    };
    const MARKER: &str = "figcon-native-acceptance-v1\n";
    #[derive(Clone)]
    pub(super) struct Anchor {
        root: PathBuf,
        device: u64,
        inode: u64,
    }
    fn directory(path: &Path) -> Result<File, &'static str> {
        if !path.is_absolute() {
            return Err("Acceptance path must be absolute");
        }
        let mut file = File::open("/").map_err(|_| "Cannot inspect acceptance root")?;
        for part in path.components().skip(1) {
            let Component::Normal(name) = part else {
                return Err("Unsafe acceptance path");
            };
            let name = CString::new(name.as_bytes()).map_err(|_| "Unsafe acceptance path")?;
            let fd = unsafe {
                libc::openat(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err("Cannot inspect acceptance directory");
            }
            file = unsafe { File::from_raw_fd(fd) };
            let m = file
                .metadata()
                .map_err(|_| "Cannot inspect acceptance directory")?;
            let uid = unsafe { libc::geteuid() };
            let sticky_system = m.uid() == 0 && m.mode() & libc::S_ISVTX as u32 != 0;
            if (m.uid() != uid && m.uid() != 0) || (m.mode() & 0o022 != 0 && !sticky_system) {
                return Err("Unsafe acceptance ancestor");
            }
        }
        let m = file
            .metadata()
            .map_err(|_| "Cannot inspect acceptance directory")?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o777 != 0o700 {
            return Err("Acceptance directories must be private");
        }
        Ok(file)
    }
    fn check(root: &Path) -> Result<File, &'static str> {
        let parent = std::fs::canonicalize(std::env::temp_dir())
            .map_err(|_| "Cannot resolve temporary directory")?;
        // Accept only a dedicated direct child of the OS temporary directory or /private/tmp.
        if root.parent() != Some(parent.as_path())
            && root.parent() != Some(Path::new("/private/tmp"))
        {
            return Err("Acceptance root must be a dedicated temporary directory");
        }
        let name = root
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid acceptance root")?;
        let suffix = name
            .strip_prefix("figcon-native-")
            .ok_or("Invalid acceptance root")?;
        if suffix.len() != 32 || !suffix.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err("Invalid acceptance root");
        }
        let file = directory(root)?;
        let name = CString::new(".figcon-native-acceptance").unwrap();
        let fd = unsafe {
            libc::openat(
                file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err("Missing acceptance marker");
        }
        let mut marker = unsafe { File::from_raw_fd(fd) };
        let m = marker.metadata().map_err(|_| "Invalid acceptance marker")?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o777 != 0o600
            || m.len() != MARKER.len() as u64
        {
            return Err("Invalid acceptance marker");
        }
        let mut text = String::new();
        marker
            .by_ref()
            .take(128)
            .read_to_string(&mut text)
            .map_err(|_| "Invalid acceptance marker")?;
        if text != MARKER {
            return Err("Invalid acceptance marker");
        }
        for name in ["home", "home/.codex", "data", "bin"] {
            directory(&root.join(name))?;
        }
        Ok(file)
    }
    impl Anchor {
        pub(super) fn verify(&self) -> Result<(), &'static str> {
            let m = check(&self.root)?
                .metadata()
                .map_err(|_| "Invalid acceptance root")?;
            if (m.dev(), m.ino()) != (self.device, self.inode) {
                return Err("Acceptance root changed");
            }
            Ok(())
        }
    }
    pub(super) fn load(root: &Path) -> Result<Roots, &'static str> {
        let m = check(root)?
            .metadata()
            .map_err(|_| "Invalid acceptance root")?;
        Ok(Roots {
            home: root.join("home"),
            data: root.join("data"),
            executables: root.join("bin"),
            anchor: Anchor {
                root: root.into(),
                device: m.dev(),
                inode: m.ino(),
            },
        })
    }
}

#[cfg(all(test, debug_assertions, target_os = "macos"))]
pub(crate) fn fixture() -> (tempfile::TempDir, Session) {
    use std::{fs, os::unix::fs::PermissionsExt};
    let temp = tempfile::Builder::new()
        .prefix("figcon-native-")
        .rand_bytes(32)
        .tempdir()
        .unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["home", "home/.codex", "data", "bin"] {
        fs::create_dir(root.join(name)).unwrap();
        fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o700)).unwrap();
    }
    let marker = root.join(".figcon-native-acceptance");
    fs::write(&marker, "figcon-native-acceptance-v1\n").unwrap();
    fs::set_permissions(marker, fs::Permissions::from_mode(0o600)).unwrap();
    let session = Session::select(Some(std::ffi::OsStr::new("1")), Some(root.as_os_str())).unwrap();
    (temp, session)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(debug_assertions))]
    #[test]
    fn release_has_no_acceptance_selection() {
        assert!(Session::from_process().unwrap().roots().unwrap().is_none());
    }
    #[cfg(debug_assertions)]
    #[test]
    fn opt_in_requires_both_values_and_never_falls_back() {
        use std::ffi::OsStr;
        assert!(
            Session::select(None, None)
                .unwrap()
                .roots()
                .unwrap()
                .is_none()
        );
        assert!(Session::select(None, Some(OsStr::new("/"))).is_err());
        assert!(Session::select(Some(OsStr::new("0")), None).is_err());
        assert!(Session::select(Some(OsStr::new("1")), None).is_err());
        assert!(Session::select(Some(OsStr::new("1")), Some(OsStr::new("/"))).is_err());
    }
    #[cfg(all(debug_assertions, target_os = "macos"))]
    #[test]
    fn acceptance_copy_cannot_downgrade_when_restart_loses_its_environment() {
        let (_temp, session) = fixture();
        let roots = session.roots().unwrap().unwrap();
        let executable = roots
            .home
            .parent()
            .unwrap()
            .join("Figcon Acceptance.app/Contents/MacOS/figcon");
        assert!(Session::select_process(None, None, &executable).is_err());
        assert!(
            Session::select_process(
                None,
                None,
                std::path::Path::new("/synthetic/project/target/debug/figcon")
            )
            .unwrap()
            .roots()
            .unwrap()
            .is_none()
        );
        let root = roots.home.parent().unwrap();
        assert!(
            Session::select_process(
                Some(std::ffi::OsStr::new("1")),
                Some(root.as_os_str()),
                &executable
            )
            .unwrap()
            .roots()
            .unwrap()
            .is_some()
        );
    }
    #[cfg(all(debug_assertions, target_os = "macos"))]
    #[test]
    fn validated_selection_overrides_tauri_storage_and_uses_ephemeral_webviews() {
        let (_temp, session) = fixture();
        let roots = session.roots().unwrap().unwrap();
        let mut config: tauri::utils::config::Config =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        session.configure(&mut config).unwrap();
        assert!(config.app.windows.iter().all(|w| w.incognito));
        assert!(
            matches!(config.app.app_directories_override, Some(tauri::utils::config::AppDirectoriesOverride::Root(p)) if p == roots.data)
        );
    }
    #[cfg(all(debug_assertions, target_os = "macos"))]
    #[test]
    fn replaced_root_symlinked_subdirectory_and_changed_marker_fail_closed() {
        use std::{fs, os::unix::fs::symlink};
        let (_temp, session) = fixture();
        let roots = session.roots().unwrap().unwrap();
        let root = roots.home.parent().unwrap();
        fs::rename(root.join("bin"), root.join("saved-bin")).unwrap();
        symlink(root.join("saved-bin"), root.join("bin")).unwrap();
        assert!(session.roots().is_err());
        fs::remove_file(root.join("bin")).unwrap();
        fs::rename(root.join("saved-bin"), root.join("bin")).unwrap();
        fs::write(root.join(".figcon-native-acceptance"), "invalid").unwrap();
        assert!(session.roots().is_err());
        let (_fresh, fresh) = fixture();
        let fresh_roots = fresh.roots().unwrap().unwrap();
        let path = fresh_roots.home.parent().unwrap();
        let moved = path.with_extension("retained");
        fs::rename(path, &moved).unwrap();
        fs::create_dir(path).unwrap();
        assert!(fresh.roots().is_err());
        fs::remove_dir(path).unwrap();
        fs::rename(moved, path).unwrap();
    }
}
