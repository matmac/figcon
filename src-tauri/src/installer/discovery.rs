use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigurationScope {
    User,
    Project,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMcpConfiguration {
    pub client_id: String,
    pub server_name: String,
    pub scope: ConfigurationScope,
    pub command: String,
    pub package_spec: Option<String>,
    pub managed_by_figcon: bool,
}

pub fn parse_claude_code_user_config(
    content: &str,
) -> Result<Vec<ExternalMcpConfiguration>, String> {
    let root: serde_json::Value = serde_json::from_str(content)
        .map_err(|_| "Invalid Claude Code configuration JSON".to_string())?;

    let mut configurations = Vec::new();

    let Some(servers) = root.get("mcpServers").and_then(|value| value.as_object()) else {
        return Ok(configurations);
    };

    for (name, server) in servers {
        let Some(command) = server.get("command").and_then(|value| value.as_str()) else {
            continue;
        };

        let package_spec = server
            .get("args")
            .and_then(|value| value.as_array())
            .and_then(|args| {
                args.iter().filter_map(|arg| arg.as_str()).find(|arg| {
                    *arg == "figma-console-mcp" || arg.starts_with("figma-console-mcp@")
                })
            });

        let is_figma_console = package_spec.is_some();

        if !is_figma_console {
            continue;
        }

        configurations.push(ExternalMcpConfiguration {
            client_id: "claude-code".to_string(),
            server_name: name.clone(),
            scope: ConfigurationScope::User,
            command: command.to_string(),
            package_spec: package_spec.map(str::to_string),
            managed_by_figcon: false,
        });
    }

    Ok(configurations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_figma_console_without_exposing_credentials() {
        let config = r#"{
            "mcpServers": {
                "figma-console": {
                    "type": "stdio",
                    "command": "npx",
                    "args": ["-y", "figma-console-mcp@latest"],
                    "env": {
                        "FIGMA_ACCESS_TOKEN": "SECRET_TEST_TOKEN"
                    }
                }
            }
        }"#;

        let result = parse_claude_code_user_config(config).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].client_id, "claude-code");
        assert_eq!(result[0].server_name, "figma-console");
        assert_eq!(
            result[0].package_spec.as_deref(),
            Some("figma-console-mcp@latest")
        );

        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("SECRET_TEST_TOKEN"));
    }

    #[test]
    fn ignores_unrelated_mcp_servers() {
        let config = r#"{
            "mcpServers": {
                "other-server": {
                    "command": "npx",
                    "args": ["-y", "unrelated-mcp"]
                }
            }
        }"#;

        let result = parse_claude_code_user_config(config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn handles_missing_mcp_servers() {
        let result = parse_claude_code_user_config("{}").unwrap();
        assert!(result.is_empty());
    }
}

#[tauri::command]
pub fn discover_claude_code_mcp() -> Result<Vec<ExternalMcpConfiguration>, String> {
    let home =
        std::env::var_os("HOME").ok_or_else(|| "Cannot resolve home directory".to_string())?;

    let config_path = std::path::PathBuf::from(home).join(".claude.json");

    if !config_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|_| "Cannot read Claude Code configuration".to_string())?;

    parse_claude_code_user_config(&content)
}
