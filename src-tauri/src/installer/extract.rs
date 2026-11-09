use flate2::read::GzDecoder;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tar::Archive;

const MAX_EXTRACTED_BYTES: u64 = 500 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;

/// Structurally extract an already checksum-verified archive. Production calls
/// this only after download_node_archive verifies the pinned manifest checksum.
/// Approved alias headers are recorded, never unpacked directly from the TAR.
pub fn extract_node_archive(
    archive_path: &Path,
    destination: &Path,
    version: &str,
    architecture: &str,
) -> Result<(), String> {
    if destination.exists() {
        return Err("Extraction destination already exists".into());
    }

    if !destination.parent().is_some_and(|parent| parent.is_dir()) {
        return Err("Extraction parent directory does not exist".into());
    }

    let prefix = format!("node-v{version}-darwin-{architecture}");

    let file = File::open(archive_path).map_err(|_| "Cannot open Node.js archive")?;

    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    fs::create_dir(destination).map_err(|_| "Cannot create extraction directory")?;

    let result = (|| -> Result<(), String> {
        let mut total_bytes = 0u64;
        let mut entry_count = 0usize;
        let mut seen_paths = HashSet::new();
        let mut regular_paths = HashSet::new();
        let mut aliases = Vec::new();

        for entry in archive.entries().map_err(|_| "Invalid archive")? {
            entry_count += 1;

            if entry_count > MAX_ENTRIES {
                return Err("Archive contains too many entries".into());
            }

            let mut entry = entry.map_err(|_| "Invalid archive entry")?;

            let path = entry
                .path()
                .map_err(|_| "Invalid archive path")?
                .into_owned();

            let mut components = path.components();

            match components.next() {
                Some(Component::Normal(root)) if root == prefix.as_str() => {}
                _ => return Err("Unexpected archive root".into()),
            }

            let mut relative = std::path::PathBuf::new();

            for component in components {
                match component {
                    Component::Normal(part) => relative.push(part),
                    _ => return Err("Unsafe archive path".into()),
                }
            }

            if !seen_paths.insert(relative.clone()) {
                return Err("Duplicate archive path".into());
            }

            let target = destination.join(&relative);
            let kind = entry.header().entry_type();

            if relative.as_os_str().is_empty() {
                if kind.is_dir() {
                    continue;
                }

                return Err("Archive root must be a directory".into());
            }

            if kind.is_dir() {
                if target.exists() && !target.is_dir() {
                    return Err("Archive directory conflicts with a file".into());
                }

                fs::create_dir_all(&target).map_err(|_| "Cannot create archive directory")?;
            } else if kind.is_file() {
                let size = entry.size();

                total_bytes = total_bytes
                    .checked_add(size)
                    .ok_or("Archive size overflow")?;

                if total_bytes > MAX_EXTRACTED_BYTES {
                    return Err("Archive exceeds extracted size limit".into());
                }

                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|_| "Cannot create file directory")?;
                }

                let mut output = File::options()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map_err(|_| "Archive file conflicts with an existing path")?;

                io::copy(&mut entry, &mut output).map_err(|_| "Cannot extract archive file")?;

                drop(output);

                #[cfg(unix)]
                {
                    let mode = entry
                        .header()
                        .mode()
                        .map_err(|_| "Invalid archive permissions")?;

                    fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o777))
                        .map_err(|_| "Cannot set extracted file permissions")?;
                }
                regular_paths.insert(relative);
            } else if kind.is_symlink() {
                if entry.size() != 0 {
                    return Err("Runtime symbolic link must not contain file data".into());
                }
                let target = entry
                    .link_name_bytes()
                    .ok_or("Missing runtime symbolic link target")?;
                aliases.push(super::runtime_aliases::from_archive(
                    entry.path_bytes().as_ref(),
                    target.as_ref(),
                    version,
                    architecture,
                )?);
            } else {
                return Err("Unsupported archive entry type".into());
            }
        }

        let node_executable = destination.join("bin/node");

        if !node_executable.is_file() {
            return Err("Node.js executable missing from archive".into());
        }

        let metadata =
            fs::metadata(&node_executable).map_err(|_| "Cannot inspect Node.js executable")?;

        if metadata.len() == 0 {
            return Err("Node.js executable is empty".into());
        }

        #[cfg(unix)]
        {
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err("Node.js executable lacks execute permission".into());
            }
        }

        for alias in &aliases {
            if !regular_paths.contains(Path::new(alias.file_target)) {
                return Err("Runtime alias target was not extracted as a regular file".into());
            }
        }
        super::runtime_aliases::reconstruct(destination, &aliases)?;

        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tar::{Builder, Header};

    #[test]
    fn extracts_valid_archive_and_preserves_permissions() {
        let temp = std::env::temp_dir().join(format!("figcon-extract-test-{}", std::process::id()));

        fs::create_dir(&temp).unwrap();

        let archive_path = temp.join("node.tar.gz");
        let destination = temp.join("runtime");

        let buffer = Vec::new();
        let encoder = GzEncoder::new(buffer, Compression::default());
        let mut builder = Builder::new(encoder);

        let content = b"fake node executable";
        let mut header = Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        builder
            .append_data(
                &mut header,
                "node-v24.22.2-darwin-arm64/bin/node",
                Cursor::new(content),
            )
            .unwrap();

        let encoder = builder.into_inner().unwrap();
        let compressed = encoder.finish().unwrap();

        fs::write(&archive_path, compressed).unwrap();

        extract_node_archive(&archive_path, &destination, "24.22.2", "arm64").unwrap();

        let extracted = destination.join("bin/node");

        assert_eq!(fs::read(&extracted).unwrap(), content);

        #[cfg(unix)]
        {
            let mode = fs::metadata(&extracted).unwrap().permissions().mode();

            assert_eq!(mode & 0o777, 0o755);
        }

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn rejects_symbolic_links_and_cleans_up() {
        let temp = std::env::temp_dir().join(format!("figcon-symlink-test-{}", std::process::id()));

        fs::create_dir_all(&temp).unwrap();

        let archive_path = temp.join("malicious.tar.gz");
        let destination = temp.join("runtime");

        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("/tmp/figcon-escape").unwrap();
        header.set_cksum();

        builder
            .append_data(
                &mut header,
                "node-v24.22.2-darwin-arm64/bin/node",
                Cursor::new(Vec::<u8>::new()),
            )
            .unwrap();

        let encoder = builder.into_inner().unwrap();
        fs::write(&archive_path, encoder.finish().unwrap()).unwrap();

        let result = extract_node_archive(&archive_path, &destination, "24.22.2", "arm64");

        assert!(result.is_err());
        assert!(!destination.exists());

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn rejects_duplicate_paths_and_cleans_up() {
        let temp =
            std::env::temp_dir().join(format!("figcon-duplicate-test-{}", std::process::id()));

        fs::create_dir_all(&temp).unwrap();

        let archive_path = temp.join("duplicate.tar.gz");
        let destination = temp.join("runtime");

        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);

        for content in [b"first".as_slice(), b"second".as_slice()] {
            let mut header = Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();

            builder
                .append_data(
                    &mut header,
                    "node-v24.22.2-darwin-arm64/bin/node",
                    Cursor::new(content),
                )
                .unwrap();
        }

        let encoder = builder.into_inner().unwrap();
        fs::write(&archive_path, encoder.finish().unwrap()).unwrap();

        let result = extract_node_archive(&archive_path, &destination, "24.22.2", "arm64");

        assert!(result.is_err());
        assert!(!destination.exists());

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn rejects_path_traversal_and_cleans_up() {
        let temp =
            std::env::temp_dir().join(format!("figcon-traversal-test-{}", std::process::id()));

        fs::create_dir_all(&temp).unwrap();

        let archive_path = temp.join("traversal.tar.gz");
        let destination = temp.join("runtime");
        let escaped_file = temp.join("escape");

        let malicious_path = "node-v24.22.2-darwin-arm64/../escape";

        let mut header = Header::new_gnu();
        header.set_size(4);
        header.set_mode(0o644);
        header.set_path("placeholder").unwrap();

        // Modify the raw TAR header to bypass the builder's
        // path validation.
        {
            let bytes = header.as_mut_bytes();
            bytes[..100].fill(0);
            bytes[..malicious_path.len()].copy_from_slice(malicious_path.as_bytes());
        }

        header.set_cksum();

        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);

        builder.append(&header, Cursor::new(b"test")).unwrap();

        let encoder = builder.into_inner().unwrap();
        fs::write(&archive_path, encoder.finish().unwrap()).unwrap();

        let result = extract_node_archive(&archive_path, &destination, "24.22.2", "arm64");

        assert!(result.is_err());
        assert!(!destination.exists());
        assert!(!escaped_file.exists());

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn rejects_hard_links_and_cleans_up() {
        let temp =
            std::env::temp_dir().join(format!("figcon-hardlink-test-{}", std::process::id()));

        fs::create_dir_all(&temp).unwrap();

        let archive_path = temp.join("hardlink.tar.gz");
        let destination = temp.join("runtime");

        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Link);
        header.set_size(0);
        header.set_mode(0o644);
        header.set_link_name("/tmp/figcon-hardlink-target").unwrap();
        header.set_cksum();

        builder
            .append_data(
                &mut header,
                "node-v24.22.2-darwin-arm64/bin/node",
                Cursor::new(Vec::<u8>::new()),
            )
            .unwrap();

        let encoder = builder.into_inner().unwrap();
        fs::write(&archive_path, encoder.finish().unwrap()).unwrap();

        let result = extract_node_archive(&archive_path, &destination, "24.22.2", "arm64");

        assert!(result.is_err());
        assert!(!destination.exists());

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn rejects_file_directory_collisions() {
        let temp =
            std::env::temp_dir().join(format!("figcon-collision-test-{}", std::process::id()));

        fs::create_dir_all(&temp).unwrap();

        let archive_path = temp.join("collision.tar.gz");
        let destination = temp.join("runtime");

        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);

        for name in [
            "node-v24.22.2-darwin-arm64/bin",
            "node-v24.22.2-darwin-arm64/bin/node",
        ] {
            let content = b"test";
            let mut header = Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            builder
                .append_data(&mut header, name, Cursor::new(content))
                .unwrap();
        }

        let encoder = builder.into_inner().unwrap();
        fs::write(&archive_path, encoder.finish().unwrap()).unwrap();

        let result = extract_node_archive(&archive_path, &destination, "24.22.2", "arm64");

        assert!(result.is_err());
        assert!(!destination.exists());

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn rejects_oversized_archive_entries() {
        let temp = std::env::temp_dir().join(format!("figcon-size-test-{}", std::process::id()));

        fs::create_dir_all(&temp).unwrap();

        let archive_path = temp.join("oversized.tar.gz");
        let destination = temp.join("runtime");

        let mut header = Header::new_gnu();
        header.set_size(MAX_EXTRACTED_BYTES + 1);
        header.set_mode(0o644);
        header
            .set_path("node-v24.22.2-darwin-arm64/bin/node")
            .unwrap();
        header.set_cksum();

        let mut raw_archive = Vec::new();
        raw_archive.extend_from_slice(header.as_bytes());
        raw_archive.extend_from_slice(&[0u8; 1024]);

        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let builder = std::io::Write::write_all;
        let mut compressed = encoder;
        builder(&mut compressed, &raw_archive).unwrap();
        fs::write(&archive_path, compressed.finish().unwrap()).unwrap();

        let result = extract_node_archive(&archive_path, &destination, "24.22.2", "arm64");

        assert!(result.is_err());
        assert!(!destination.exists());

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn handles_archive_root_entries() {
        for is_directory in [true, false] {
            let temp = std::env::temp_dir().join(format!(
                "figcon-root-test-{}-{}",
                std::process::id(),
                is_directory
            ));

            fs::create_dir_all(&temp).unwrap();

            let archive_path = temp.join("root.tar.gz");
            let destination = temp.join("runtime");

            let encoder = GzEncoder::new(Vec::new(), Compression::default());
            let mut builder = Builder::new(encoder);

            let mut header = Header::new_gnu();
            header.set_entry_type(if is_directory {
                tar::EntryType::Directory
            } else {
                tar::EntryType::Regular
            });
            header.set_size(0);
            header.set_mode(0o755);
            header.set_cksum();

            builder
                .append_data(
                    &mut header,
                    "node-v24.22.2-darwin-arm64/",
                    Cursor::new(Vec::<u8>::new()),
                )
                .unwrap();

            let encoder = builder.into_inner().unwrap();
            fs::write(&archive_path, encoder.finish().unwrap()).unwrap();

            let result = extract_node_archive(&archive_path, &destination, "24.22.2", "arm64");

            // Both archives are incomplete because neither contains bin/node.
            // A root directory entry is structurally valid, but an incomplete
            // Node.js installation must still be rejected.
            assert!(result.is_err());
            assert!(!destination.exists());

            fs::remove_dir_all(temp).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_node_without_execute_permission() {
        let temp =
            std::env::temp_dir().join(format!("figcon-nonexecutable-test-{}", std::process::id()));

        fs::create_dir_all(&temp).unwrap();

        let archive_path = temp.join("node.tar.gz");
        let destination = temp.join("runtime");

        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);

        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(4);
        header.set_mode(0o644);
        header.set_cksum();

        builder
            .append_data(
                &mut header,
                "node-v24.22.2-darwin-arm64/bin/node",
                Cursor::new(b"node".to_vec()),
            )
            .unwrap();

        let encoder = builder.into_inner().unwrap();
        fs::write(&archive_path, encoder.finish().unwrap()).unwrap();

        let result = extract_node_archive(&archive_path, &destination, "24.22.2", "arm64");

        assert!(result.is_err());
        assert!(!destination.exists());

        fs::remove_dir_all(temp).unwrap();
    }
}
