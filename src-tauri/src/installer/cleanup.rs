use std::fs;
use std::io;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub fn remove_if_same_file(path: &Path, original: &fs::Metadata) -> io::Result<bool> {
    let current = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };

    #[cfg(unix)]
    {
        if !current.is_file() || current.dev() != original.dev() || current.ino() != original.ino()
        {
            return Ok(false);
        }

        fs::remove_file(path)?;
        Ok(true)
    }

    #[cfg(not(unix))]
    {
        let _ = current;
        let _ = original;

        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Safe cleanup is not implemented on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn removes_original_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.tar.gz");

        fs::write(&path, b"original").unwrap();
        let original = fs::metadata(&path).unwrap();

        assert!(remove_if_same_file(&path, &original).unwrap());
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn preserves_replacement_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.tar.gz");
        let old = dir.path().join("old.tar.gz");

        fs::write(&path, b"original").unwrap();
        let original = fs::metadata(&path).unwrap();

        fs::rename(&path, &old).unwrap();
        fs::write(&path, b"replacement").unwrap();

        assert!(!remove_if_same_file(&path, &original).unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
    }
}
