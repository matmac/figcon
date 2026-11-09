use std::path::{Component, Path};

use serde_json::Value;

use super::manifest::{MCP_ENTRY_POINT, MCP_INTEGRITY, MCP_TARBALL, MCP_VERSION};

pub(super) const PACKAGE_JSON: &str = include_str!("../../../installer/npm/package.json");
pub(super) const PACKAGE_LOCK: &str = include_str!("../../../installer/npm/package-lock.json");

pub(super) fn exact_version(version: &str) -> bool {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<_> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
        && version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c))
}

fn integrity(value: &str) -> bool {
    value.strip_prefix("sha512-").is_some_and(|hash| {
        hash.len() == 88
            && hash.ends_with("==")
            && hash.as_bytes()[..86]
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || b"+/".contains(c))
    })
}

fn registry_tarball(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("registry.npmjs.org")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path().ends_with(".tgz")
            && !value.contains('%')
            && !value.split('/').any(|part| part == ".." || part == ".")
    })
}

pub(super) fn validate_contract(package: &str, lock: &str) -> Result<Value, String> {
    let package: Value =
        serde_json::from_str(package).map_err(|_| "Invalid pinned package manifest")?;
    let lock: Value = serde_json::from_str(lock).map_err(|_| "Invalid pinned package lockfile")?;
    let expected = serde_json::json!({"figma-console-mcp": MCP_VERSION});
    if package["dependencies"] != expected
        || package["private"] != true
        || package.get("scripts").is_some()
        || package.get("workspaces").is_some()
        || ["optionalDependencies", "devDependencies", "overrides"]
            .iter()
            .any(|field| package.get(field).is_some())
        || lock["lockfileVersion"] != 3
        || lock["packages"][""]["dependencies"] != expected
        || package["name"] != lock["packages"][""]["name"]
        || package["version"] != lock["packages"][""]["version"]
    {
        return Err("Pinned package manifest and lockfile disagree".into());
    }
    let packages = lock["packages"]
        .as_object()
        .ok_or("Missing locked packages")?;
    for (path, entry) in packages.iter().filter(|(path, _)| !path.is_empty()) {
        if !path.starts_with("node_modules/")
            || path.contains('\\')
            || !Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            || !entry["version"].as_str().is_some_and(exact_version)
            || !entry["resolved"].as_str().is_some_and(registry_tarball)
            || !entry["integrity"].as_str().is_some_and(integrity)
            || entry.get("link").is_some()
            || entry.get("optional").is_some()
            || entry.get("dev").is_some()
            || entry.get("inBundle").is_some()
        {
            return Err("Lockfile contains an unsupported or unpinned package".into());
        }
    }
    let mcp = &lock["packages"]["node_modules/figma-console-mcp"];
    if mcp["version"] != MCP_VERSION
        || mcp["resolved"] != MCP_TARBALL
        || mcp["integrity"] != MCP_INTEGRITY
        || mcp["bin"]["figma-console-mcp"] != MCP_ENTRY_POINT
    {
        return Err("Locked MCP package does not match the pinned contract".into());
    }
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_contract_is_pinned() {
        assert!(validate_contract(PACKAGE_JSON, PACKAGE_LOCK).is_ok());
    }

    #[test]
    fn rejects_changed_contract_and_unsafe_dependency_sources() {
        for replacement in ["1.46.1", "^1.46.0"] {
            assert!(
                validate_contract(&PACKAGE_JSON.replace("1.46.0", replacement), PACKAGE_LOCK)
                    .is_err()
            );
        }
        let original: Value = serde_json::from_str(PACKAGE_LOCK).unwrap();
        for (key, value) in [
            ("version", "latest"),
            ("integrity", "sha512-invalid"),
            ("resolved", "https://example.com/package.tgz"),
            ("resolved", "file:../escape"),
        ] {
            let mut lock = original.clone();
            lock["packages"]["node_modules/figma-console-mcp"][key] = value.into();
            assert!(validate_contract(PACKAGE_JSON, &lock.to_string()).is_err());
        }
    }
}
