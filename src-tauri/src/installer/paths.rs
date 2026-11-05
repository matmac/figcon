use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct InstallerPaths {
    pub root: PathBuf,
    pub runtime: PathBuf,
    pub staging: PathBuf,
    pub mcp: PathBuf,
    pub metadata: PathBuf,
}

impl InstallerPaths {
    pub fn new(root: PathBuf) -> Self {
        Self {
            runtime: root.join("runtime"),
            staging: root.join("staging"),
            mcp: root.join("mcp"),
            metadata: root.join("metadata"),
            root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_installer_directories() {
        let paths = InstallerPaths::new(PathBuf::from("/tmp/figcon-test"));

        assert_eq!(paths.root, PathBuf::from("/tmp/figcon-test"));
        assert_eq!(paths.runtime, paths.root.join("runtime"));
        assert_eq!(paths.staging, paths.root.join("staging"));
        assert_eq!(paths.mcp, paths.root.join("mcp"));
        assert_eq!(paths.metadata, paths.root.join("metadata"));
    }
}
