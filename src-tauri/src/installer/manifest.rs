// Pinned, verified dependencies for Figcon's managed installation.

pub const NODE_VERSION: &str = "24.22.2";
pub const MCP_VERSION: &str = "1.46.0";

pub const NODE_ARM64_SHA256: &str =
    "aa9549349e47dffaa42dcf95d6e06ade8c4bdafc980979883b88767670df385a";

pub const NODE_X64_SHA256: &str =
    "d5e336d588c45be0c37dbd52cc29895593009ca4997a77a1fd02815131ed0e52";

pub const MCP_INTEGRITY: &str = "sha512-gOzHXHYa/4yYIddjW1CSbU8JCUqkfcSnA8uBz8Q1SjLDt3yeLwYEik4Rs0rtK9PtFxZd0uFFeRMEf5NuLwM57Q==";

pub fn node_archive(architecture: &str) -> Option<(String, &'static str)> {
    let (filename, checksum) = match architecture {
        "arm64" => (
            format!("node-v{NODE_VERSION}-darwin-arm64.tar.gz"),
            NODE_ARM64_SHA256,
        ),
        "x64" => (
            format!("node-v{NODE_VERSION}-darwin-x64.tar.gz"),
            NODE_X64_SHA256,
        ),
        _ => return None,
    };

    Some((
        format!("https://nodejs.org/dist/v{NODE_VERSION}/{filename}"),
        checksum,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_apple_silicon_runtime() {
        let (url, checksum) = node_archive("arm64").unwrap();

        assert!(url.ends_with("darwin-arm64.tar.gz"));
        assert_eq!(checksum, NODE_ARM64_SHA256);
    }

    #[test]
    fn resolves_intel_runtime() {
        let (url, checksum) = node_archive("x64").unwrap();

        assert!(url.ends_with("darwin-x64.tar.gz"));
        assert_eq!(checksum, NODE_X64_SHA256);
    }

    #[test]
    fn rejects_unsupported_architecture() {
        assert!(node_archive("unsupported").is_none());
    }
}
