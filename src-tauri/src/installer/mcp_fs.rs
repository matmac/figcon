use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::manifest::{MCP_ENTRY_POINT, MCP_VERSION};
use super::mcp_contract::{PACKAGE_JSON, PACKAGE_LOCK};

pub(super) fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| "Cannot open managed metadata")?;
    let metadata = file
        .metadata()
        .map_err(|_| "Cannot inspect managed metadata")?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("Managed metadata is unsafe or exceeds limit".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err("Managed metadata must not be hard linked".into());
        }
    }
    let mut contents = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut contents)
        .map_err(|_| "Cannot read managed metadata")?;
    if contents.len() as u64 > limit {
        return Err("Managed metadata exceeds limit".into());
    }
    Ok(contents)
}

pub(super) fn real_file(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    let metadata = fs::symlink_metadata(&path).map_err(|_| "Managed directory is missing")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Managed directory must be a real directory".into());
    }
    for component in relative.components() {
        let Component::Normal(part) = component else {
            return Err("Unsafe managed file path".into());
        };
        path.push(part);
        let metadata = fs::symlink_metadata(&path).map_err(|_| "Managed file is missing")?;
        if metadata.file_type().is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            return Err("Managed path contains an unsafe filesystem entry".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.is_file() && metadata.nlink() != 1 {
                return Err("Managed files must not be hard linked".into());
            }
        }
    }
    if !fs::symlink_metadata(&path)
        .map_err(|_| "Cannot inspect managed file")?
        .is_file()
    {
        return Err("Managed file is not a regular file".into());
    }
    Ok(path)
}

pub(super) fn private_directory(path: &Path) -> Result<(), String> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|_| "Cannot create private installation directory".into())
}

pub(super) fn write_new(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "Cannot create pinned installation file")?;
    file.write_all(contents)
        .map_err(|_| "Cannot write pinned installation file".into())
}

// Scan before reading package metadata or running npm ls: no package-owned
// symlinks (including .bin), devices, or hard links are accepted.
pub(super) fn validate_tree(root: &Path) -> Result<(), String> {
    validate_tree_with_links(root, |_| {
        Err("Installed tree contains a symbolic link".into())
    })
}

// Callers must explicitly supply a policy for every symlink. The default
// validator above always rejects them; only the runtime's fixed aliases opt in.
pub(super) fn validate_tree_with_links(
    root: &Path,
    validate_link: impl Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let mut pending = vec![root.to_path_buf()];
    let mut entries = 0usize;
    let mut bytes = 0u64;
    while let Some(path) = pending.pop() {
        entries += 1;
        let metadata = fs::symlink_metadata(&path).map_err(|_| "Cannot inspect installed tree")?;
        if entries > 200_000 {
            return Err("Installed tree contains unsafe entries or exceeds limits".into());
        }
        if metadata.file_type().is_symlink() {
            validate_link(&path)?;
            continue;
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(path).map_err(|_| "Cannot inspect installed directory")? {
                pending.push(entry.map_err(|_| "Cannot inspect installed entry")?.path());
                if pending.len() + entries > 200_000 {
                    return Err("Installed tree exceeds entry limit".into());
                }
            }
        } else if metadata.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() != 1 {
                    return Err("Installed tree contains hard links".into());
                }
            }
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or("Installed tree size overflow")?;
            if bytes > 2 * 1024 * 1024 * 1024 {
                return Err("Installed tree exceeds size limit".into());
            }
        } else {
            return Err("Installed tree contains an unexpected filesystem entry".into());
        }
    }
    Ok(())
}

pub(super) fn validate_installation(root: &Path, lock: &Value) -> Result<(), String> {
    validate_tree(root)?;
    for entry in fs::read_dir(root).map_err(|_| "Cannot inspect MCP project")? {
        let name = entry
            .map_err(|_| "Cannot inspect MCP project entry")?
            .file_name();
        if name != "package.json" && name != "package-lock.json" && name != "node_modules" {
            return Err("Unexpected MCP project entry".into());
        }
    }
    for (name, expected) in [
        ("package.json", PACKAGE_JSON),
        ("package-lock.json", PACKAGE_LOCK),
    ] {
        let actual = read_regular(&real_file(root, Path::new(name))?, expected.len() as u64)?;
        if actual != expected.as_bytes() {
            return Err("npm modified a pinned manifest or lockfile".into());
        }
    }
    for (path, locked) in lock["packages"]
        .as_object()
        .ok_or("Invalid lockfile")?
        .iter()
        .filter(|(p, _)| !p.is_empty())
    {
        let metadata = real_file(root, &Path::new(path).join("package.json"))?;
        let installed: Value = serde_json::from_slice(&read_regular(&metadata, 1024 * 1024)?)
            .map_err(|_| "Invalid installed package metadata")?;
        let name = path
            .rsplit("node_modules/")
            .next()
            .ok_or("Invalid locked package path")?;
        if installed["version"] != locked["version"] || installed["name"] != name {
            return Err("Installed dependency version differs from lockfile".into());
        }
        for field in [
            "dependencies",
            "optionalDependencies",
            "peerDependencies",
            "peerDependenciesMeta",
        ] {
            // npm omits empty dependency maps from lockfile v3. Treat an
            // absent map and {} identically, while comparing every real edge.
            let actual = installed
                .get(field)
                .and_then(Value::as_object)
                .filter(|map| !map.is_empty());
            let expected = locked
                .get(field)
                .and_then(Value::as_object)
                .filter(|map| !map.is_empty());
            if installed.get(field).is_some_and(|value| !value.is_object()) || actual != expected {
                return Err("Installed dependency metadata differs from lockfile".into());
            }
        }
        if path == "node_modules/figma-console-mcp"
            && (installed["name"] != "figma-console-mcp"
                || installed["version"] != MCP_VERSION
                || !matches_mcp_bin(&installed["bin"]))
        {
            return Err("Installed MCP package differs from pinned contract".into());
        }
    }
    let expected: std::collections::BTreeSet<_> = lock["packages"]
        .as_object()
        .ok_or("Invalid lockfile")?
        .keys()
        .filter(|path| !path.is_empty())
        .cloned()
        .collect();
    if installed_package_paths(root)? != expected {
        return Err("Installed tree has missing or extraneous packages".into());
    }
    let entry = real_file(
        root,
        &Path::new("node_modules/figma-console-mcp").join(MCP_ENTRY_POINT),
    )?;
    if fs::metadata(entry)
        .map_err(|_| "Cannot inspect MCP entry point")?
        .len()
        == 0
    {
        return Err("MCP entry point is empty".into());
    }
    Ok(())
}

fn installed_package_paths(root: &Path) -> Result<std::collections::BTreeSet<String>, String> {
    let mut found = std::collections::BTreeSet::new();
    let mut modules = vec![root.join("node_modules")];
    while let Some(directory) = modules.pop() {
        let mut packages = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|_| "Missing installed dependencies")? {
            let entry = entry.map_err(|_| "Cannot inspect dependency tree")?;
            let name = entry.file_name();
            if name == ".package-lock.json" {
                continue;
            }
            if name.to_string_lossy().starts_with('@') {
                for scoped in fs::read_dir(entry.path()).map_err(|_| "Invalid installed scope")? {
                    packages.push(scoped.map_err(|_| "Invalid scoped dependency")?.path());
                }
            } else {
                packages.push(entry.path());
            }
        }
        for package in packages {
            let path = package
                .strip_prefix(root)
                .map_err(|_| "Dependency escaped staging")?
                .to_string_lossy()
                .replace('\\', "/");
            found.insert(path);
            if package.join("node_modules").exists() {
                modules.push(package.join("node_modules"));
            }
        }
    }
    Ok(found)
}

// npm normalizes published bin declarations when recording lockfile metadata,
// but preserves their original string/map and ./ form in extracted manifests.
// Accept only the exact contracted relative path after this normalization.
fn matches_mcp_bin(bin: &Value) -> bool {
    let entry = bin
        .as_str()
        .or_else(|| bin.get("figma-console-mcp").and_then(Value::as_str));
    entry.is_some_and(|entry| entry.strip_prefix("./").unwrap_or(entry) == MCP_ENTRY_POINT)
}
