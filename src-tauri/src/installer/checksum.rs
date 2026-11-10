use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub fn verify_sha256(path: &Path, expected: &str) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        super::deadline::check().map_err(io::Error::other)?;
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let actual = format!("{:x}", hasher.finalize());

    Ok(actual.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn accepts_valid_checksum() {
        let path =
            std::env::temp_dir().join(format!("figcon-checksum-valid-{}", std::process::id()));

        let mut file = File::create(&path).unwrap();
        file.write_all(b"abc").unwrap();
        drop(file);

        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

        assert!(verify_sha256(&path, expected).unwrap());

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_invalid_checksum() {
        let path =
            std::env::temp_dir().join(format!("figcon-checksum-invalid-{}", std::process::id()));

        let mut file = File::create(&path).unwrap();
        file.write_all(b"modified content").unwrap();
        drop(file);

        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

        assert!(!verify_sha256(&path, expected).unwrap());

        std::fs::remove_file(path).unwrap();
    }
}
