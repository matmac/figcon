use std::path::Path;

use tokio::io::AsyncWriteExt;

use super::checksum::verify_sha256;
use super::manifest::node_archive;

const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;

pub async fn download_node_archive(architecture: &str, destination: &Path) -> Result<(), String> {
    super::deadline::check()?;
    let (url, expected_checksum) =
        node_archive(architecture).ok_or("Unsupported macOS architecture")?;

    if destination.exists() {
        return Err("Download destination already exists".into());
    }

    if !destination.parent().is_some_and(|parent| parent.is_dir()) {
        return Err("Download destination directory does not exist".into());
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|_| "Failed to initialize downloader")?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "Node.js download failed")?;

    if !response.status().is_success() {
        return Err("Node.js download returned an unsuccessful status".into());
    }

    if let Some(length) = response.content_length() {
        if length > MAX_ARCHIVE_BYTES {
            return Err("Node.js archive exceeds the maximum allowed size".into());
        }
    }

    let result: Result<(), String> = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .await
            .map_err(|_| "Failed to create download file")?;

        if !file
            .metadata()
            .await
            .map_err(|_| "Failed to inspect download file")?
            .is_file()
        {
            return Err("Download destination is not a regular file".into());
        }

        let mut response = response;
        let mut total_bytes = 0u64;

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Node.js download interrupted")?
        {
            super::deadline::check()?;
            total_bytes = total_bytes
                .checked_add(chunk.len() as u64)
                .ok_or("Download size overflow")?;

            if total_bytes > MAX_ARCHIVE_BYTES {
                return Err("Node.js archive exceeds the maximum allowed size".into());
            }

            file.write_all(&chunk)
                .await
                .map_err(|_| "Failed to write download file")?;
        }

        if total_bytes == 0 {
            return Err("Node.js download was empty".into());
        }

        file.flush()
            .await
            .map_err(|_| "Failed to flush download file")?;

        drop(file);

        let valid = verify_sha256(destination, expected_checksum)
            .map_err(|_| "Failed to verify archive checksum")?;

        if !valid {
            return Err("Node.js archive checksum mismatch".into());
        }

        Ok(())
    }
    .await;

    if let Err(error) = result {
        return Err(format!(
            "{error}; download staging retained; automatic cleanup disabled"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_unsupported_architecture() {
        let destination = std::env::temp_dir().join("figcon-unsupported-architecture-test.tar.gz");

        let result = download_node_archive("unsupported", &destination).await;

        assert_eq!(result.unwrap_err(), "Unsupported macOS architecture");
    }

    #[tokio::test]
    async fn refuses_to_overwrite_existing_file() {
        let destination = std::env::temp_dir().join(format!(
            "figcon-existing-download-{}.tar.gz",
            std::process::id()
        ));

        std::fs::write(&destination, b"existing content").unwrap();

        let result = download_node_archive("arm64", &destination).await;

        assert_eq!(result.unwrap_err(), "Download destination already exists");

        assert_eq!(std::fs::read(&destination).unwrap(), b"existing content");

        std::fs::remove_file(destination).unwrap();
    }
}
