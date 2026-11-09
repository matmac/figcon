use std::ffi::OsStr;
use std::path::Path;

use super::manifest::NODE_VERSION;
use super::mcp_fs::real_file;

#[derive(PartialEq, Eq)]
pub(super) struct RuntimeAlias {
    pub name: &'static str,
    pub link_target: &'static str,
    pub file_target: &'static str,
}

// Exact entries inspected in BOTH checksum-verified Node.js 24.22.2 macOS
// archives. Never feed an arbitrary archive link target to symlink().
pub(super) const APPROVED: [RuntimeAlias; 3] = [
    RuntimeAlias {
        name: "bin/npm",
        link_target: "../lib/node_modules/npm/bin/npm-cli.js",
        file_target: "lib/node_modules/npm/bin/npm-cli.js",
    },
    RuntimeAlias {
        name: "bin/npx",
        link_target: "../lib/node_modules/npm/bin/npx-cli.js",
        file_target: "lib/node_modules/npm/bin/npx-cli.js",
    },
    RuntimeAlias {
        name: "bin/corepack",
        link_target: "../lib/node_modules/corepack/dist/corepack.js",
        file_target: "lib/node_modules/corepack/dist/corepack.js",
    },
];

pub(super) fn from_archive(
    path: &[u8],
    target: &[u8],
    version: &str,
    architecture: &str,
) -> Result<&'static RuntimeAlias, String> {
    if version != NODE_VERSION || !matches!(architecture, "arm64" | "x64") {
        return Err("Executable aliases are only approved for the pinned macOS runtime".into());
    }
    APPROVED
        .iter()
        .find(|alias| {
            path == format!("node-v{NODE_VERSION}-darwin-{architecture}/{}", alias.name).as_bytes()
                && target == alias.link_target.as_bytes()
        })
        .ok_or_else(|| "Unexpected runtime symbolic link name or target".into())
}

fn validate_target(root: &Path, alias: &RuntimeAlias) -> Result<(), String> {
    let target = real_file(root, Path::new(alias.file_target))?;
    if std::fs::symlink_metadata(target)
        .map_err(|_| "Cannot inspect runtime alias target")?
        .len()
        == 0
    {
        return Err("Runtime alias target must be a nonempty regular file".into());
    }
    Ok(())
}

pub(super) fn reconstruct(root: &Path, aliases: &[&RuntimeAlias]) -> Result<(), String> {
    // Validate every target before creating any link. Targets and each of their
    // ancestors must be real, in-runtime filesystem entries, never other links.
    for alias in aliases {
        if !APPROVED.contains(alias) {
            return Err("Cannot reconstruct an unapproved runtime alias".into());
        }
        validate_target(root, alias)?;
    }
    for alias in aliases {
        let guard = super::cleanup::DirectoryGuard::capture(&[root, &root.join("bin")])
            .map_err(|_| "Runtime alias parent must be a real directory")?;
        validate_target(root, alias)?;
        guard
            .verify()
            .map_err(|_| "Runtime alias parent identity changed")?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(alias.link_target, root.join(alias.name))
            .map_err(|_| "Cannot reconstruct runtime executable alias")?;
        #[cfg(not(unix))]
        return Err("Runtime executable alias reconstruction requires Unix".into());
    }
    Ok(())
}

// Runtime-only exception. The generic MCP tree validator remains link-free.
pub(super) fn validate_runtime_tree(root: &Path) -> Result<(), String> {
    super::mcp_fs::validate_tree_with_links(root, |path| {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "Runtime alias escaped managed root")?;
        let alias = APPROVED
            .iter()
            .find(|alias| relative.as_os_str() == OsStr::new(alias.name))
            .ok_or("Unexpected runtime symbolic link")?;
        let target =
            std::fs::read_link(path).map_err(|_| "Cannot inspect runtime symbolic link")?;
        if target.as_os_str() != OsStr::new(alias.link_target) {
            return Err("Unexpected runtime symbolic link target".into());
        }
        // Ensure bin itself has not been replaced with a link.
        real_file(root, Path::new("bin/node"))?;
        validate_target(root, alias)
    })
}

#[cfg(test)]
#[path = "runtime_aliases_tests.rs"]
mod tests;
