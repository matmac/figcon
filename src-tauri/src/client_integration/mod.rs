//! Read-only integration foundation. No client subprocesses, writes, or approval tokens.
mod claude;
mod consent;
mod consent_models;
pub use consent::Service as ConsentService;
pub use consent_models::*;
mod codex;
mod models;
mod ownership;
mod read;
// The production engine constructor supports read-only planning; mutation stays gated.
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod write_engine;
use crate::installer::{paths::InstallerPaths, status::inspect_installation};
pub use models::*;
use serde_json::Value;
use std::path::PathBuf;
use tauri::Manager;
const SERVER_NAME: &str = "figcon-figma-console";
fn production_search_roots(home: &std::path::Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".local/bin"),
        home.join(".npm-global/bin"),
    ]
}
#[derive(Clone)]
struct DiscoveryEnvironment {
    home: PathBuf,
    confined: bool,
    managed: InstallerPaths,
    supported: bool,
    claude_override: bool,
    codex_override: bool,
    // Backend-generated candidates only. Never supplied through IPC.
    claude_candidates: Vec<PathBuf>,
    codex_candidates: Vec<PathBuf>,
}
type Context = DiscoveryEnvironment;
impl DiscoveryEnvironment {
    fn retained_bytes(&self) -> usize {
        [
            &self.home,
            &self.managed.root,
            &self.managed.runtime,
            &self.managed.mcp,
            &self.managed.staging,
            &self.managed.metadata,
        ]
        .iter()
        .map(|p| p.as_os_str().len())
        .sum::<usize>()
            + self
                .claude_candidates
                .iter()
                .chain(&self.codex_candidates)
                .map(|p| p.as_os_str().len())
                .sum::<usize>()
            + 2048
    }
    fn production(app: &tauri::AppHandle) -> Result<Self, InspectionError> {
        if let Some(roots) =
            crate::acceptance::roots(app).map_err(|_| InspectionError::unavailable())?
        {
            return Ok(Self::isolated(roots));
        }
        let home = app
            .path()
            .home_dir()
            .map_err(|_| InspectionError::unavailable())?;
        let data = app
            .path()
            .app_data_dir()
            .map_err(|_| InspectionError::unavailable())?;
        let roots = production_search_roots(&home);
        Ok(Self::from_roots(
            home,
            data,
            roots,
            cfg!(target_os = "macos"),
            std::env::var_os("CLAUDE_CONFIG_DIR").is_some(),
            std::env::var_os("CODEX_HOME").is_some(),
        ))
    }
    fn isolated(roots: crate::acceptance::Roots) -> Self {
        let mut environment = Self::from_roots(
            roots.home,
            roots.data,
            vec![roots.executables],
            cfg!(target_os = "macos"),
            false,
            false,
        );
        environment.confined = true;
        environment
    }
    fn from_roots(
        home: PathBuf,
        data: PathBuf,
        executable_roots: Vec<PathBuf>,
        supported: bool,
        claude_override: bool,
        codex_override: bool,
    ) -> Self {
        Self {
            home,
            confined: false,
            managed: InstallerPaths::new(data.join("figcon-installer")),
            supported,
            claude_override,
            codex_override,
            claude_candidates: executable_roots.iter().map(|p| p.join("claude")).collect(),
            codex_candidates: executable_roots.iter().map(|p| p.join("codex")).collect(),
        }
    }
    fn config(&self, client: Client) -> PathBuf {
        self.home.join(match client {
            Client::ClaudeCode => claude::ClaudeCodeAdapter::CONFIGURATION,
            Client::CodexCli => codex::CodexCliAdapter::CONFIGURATION,
        })
    }
    fn launch(&self) -> (String, Vec<String>) {
        (
            self.managed
                .runtime
                .join("bin/node")
                .to_string_lossy()
                .into_owned(),
            vec![
                self.managed
                    .mcp
                    .join("node_modules/figma-console-mcp/dist/local.js")
                    .to_string_lossy()
                    .into_owned(),
            ],
        )
    }
    fn detection(&self, client: Client) -> DetectionStatus {
        if !self.supported {
            return DetectionStatus::UnsupportedPlatform;
        }
        let paths = match client {
            Client::ClaudeCode => &self.claude_candidates,
            Client::CodexCli => &self.codex_candidates,
        };
        let mut found = std::collections::BTreeSet::new();
        for path in paths {
            if self.confined {
                if read::synthetic_executable(path) {
                    found.insert(path.clone());
                }
                continue;
            }
            let Ok(real) = std::fs::canonicalize(path) else {
                continue;
            };
            let Ok(meta) = std::fs::metadata(&real) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if meta.mode() & 0o111 == 0
                    || meta.mode() & 0o022 != 0
                    || (meta.uid() != 0 && meta.uid() != unsafe { libc::geteuid() })
                {
                    continue;
                }
            }
            found.insert(real);
        }
        match found.len() {
            0 => DetectionStatus::Missing,
            1 => DetectionStatus::Detected,
            _ => DetectionStatus::Ambiguous,
        }
    }
}
fn related(name: &str, entry: &Value) -> bool {
    name == SERVER_NAME
        || name == "figma-console"
        || entry
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| {
                command == "figma-console-mcp" || command.ends_with("/figma-console-mcp")
            })
        || entry
            .get("args")
            .and_then(Value::as_array)
            .is_some_and(|a| {
                a.iter().filter_map(Value::as_str).any(|s| {
                    s == "figma-console-mcp"
                        || s.starts_with("figma-console-mcp@")
                        || s.ends_with("/node_modules/figma-console-mcp/dist/local.js")
                })
            })
}
fn classify(
    root: &Value,
    client: Client,
    context: &Context,
    warnings: &mut Vec<Warning>,
) -> RegistrationStatus {
    let key = match client {
        Client::ClaudeCode => "mcpServers",
        Client::CodexCli => "mcp_servers",
    };
    let Some(servers) = root.get(key) else {
        return RegistrationStatus::NotRegistered;
    };
    let Some(servers) = servers.as_object() else {
        warnings.push(Warning::UnsupportedRegistration);
        return RegistrationStatus::Unsupported;
    };
    let related: Vec<_> = servers
        .iter()
        .filter(|(name, v)| related(name, v))
        .collect();
    if related.iter().any(|(name, _)| name.as_str() != SERVER_NAME) {
        warnings.push(Warning::AdditionalRegistration);
    }
    let Some(entry) = servers.get(SERVER_NAME) else {
        return if related.is_empty() {
            RegistrationStatus::NotRegistered
        } else {
            RegistrationStatus::External
        };
    };
    let Some(fields) = entry.as_object() else {
        warnings.push(Warning::UnsupportedRegistration);
        return RegistrationStatus::Unsupported;
    };
    let allowed: &[&str] = match client {
        Client::ClaudeCode => &["type", "command", "args"],
        Client::CodexCli => &["command", "args"],
    };
    if fields.keys().any(|k| !allowed.contains(&k.as_str())) {
        warnings.push(Warning::UnsupportedRegistration);
        return RegistrationStatus::Unsupported;
    }
    if client == Client::ClaudeCode && entry.get("type").is_some_and(|v| v != "stdio") {
        return RegistrationStatus::Conflict;
    }
    let (command, args) = context.launch();
    if entry.get("command") == Some(&Value::String(command))
        && entry.get("args") == Some(&serde_json::json!(args))
    {
        warnings.push(Warning::OwnershipUnverified);
        RegistrationStatus::MatchingExternal
    } else {
        RegistrationStatus::Conflict
    }
}
fn inspect(context: &Context, client: Client) -> Inspection {
    let detection = context.detection(client);
    let mut warnings = vec![Warning::ReadOnly, Warning::EffectiveConfigurationUnknown];
    match detection {
        DetectionStatus::Missing => warnings.push(Warning::ClientMissing),
        DetectionStatus::Ambiguous => warnings.push(Warning::ClientAmbiguous),
        DetectionStatus::UnsupportedPlatform => warnings.push(Warning::UnsupportedPlatform),
        _ => {}
    }
    let overridden = match client {
        Client::ClaudeCode => context.claude_override,
        Client::CodexCli => context.codex_override,
    };
    let mut registration = RegistrationStatus::Unknown;
    let configuration = if !context.supported {
        ConfigurationStatus::UnsupportedPlatform
    } else if overridden {
        ConfigurationStatus::UnsupportedLocation
    } else {
        match read::read(&context.config(client)) {
            Ok(text) => match match client {
                Client::ClaudeCode => claude::ClaudeCodeAdapter::inspect(&text),
                Client::CodexCli => codex::CodexCliAdapter::inspect(&text),
            } {
                Ok(root) => {
                    if client == Client::ClaudeCode && claude::shadowed(&root) {
                        warnings.push(Warning::PossibleShadowing);
                    }
                    registration = classify(&root, client, context, &mut warnings);
                    ConfigurationStatus::Readable
                }
                Err(()) => ConfigurationStatus::Malformed,
            },
            Err(ConfigurationStatus::Missing) => {
                registration = RegistrationStatus::NotRegistered;
                ConfigurationStatus::Missing
            }
            Err(status) => status,
        }
    };
    match configuration {
        ConfigurationStatus::UnsupportedLocation => warnings.push(Warning::UnsupportedLocation),
        ConfigurationStatus::Malformed => warnings.push(Warning::MalformedConfiguration),
        ConfigurationStatus::Unsafe => warnings.push(Warning::UnsafeConfiguration),
        ConfigurationStatus::Inaccessible => warnings.push(Warning::InaccessibleConfiguration),
        _ => {}
    }
    if registration == RegistrationStatus::Conflict {
        warnings.push(Warning::RegistrationConflict);
    }
    Inspection {
        client,
        client_name: client.name().into(),
        detection,
        configuration,
        configuration_location: match client {
            Client::ClaudeCode => "~/.claude.json",
            Client::CodexCli => "~/.codex/config.toml",
        }
        .into(),
        registration,
        ownership: ownership::assess(None, client),
        warnings,
    }
}
fn preview(context: &Context, client: Client) -> ConfigurationPreview {
    let mut inspection = inspect(context, client);
    let valid = inspect_installation(&context.managed).state
        == crate::installer::models::InstallationState::Installed;
    if !valid {
        inspection
            .warnings
            .push(Warning::ManagedInstallationUnavailable);
    }
    let (proposed_executable, proposed_arguments) = context.launch();
    ConfigurationPreview {
        inspection,
        proposed_server_name: SERVER_NAME.into(),
        proposed_executable,
        proposed_arguments,
        scope: "user".into(),
        existing_registration_would_be_affected: false,
        approval_required: true,
        safe_to_apply: false,
        managed_installation_valid: valid,
    }
}
#[tauri::command]
pub async fn list_ai_clients(app: tauri::AppHandle) -> Result<Vec<Inspection>, InspectionError> {
    let context = Context::production(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        vec![
            inspect(&context, Client::ClaudeCode),
            inspect(&context, Client::CodexCli),
        ]
    })
    .await
    .map_err(|_| InspectionError::unavailable())
}
#[tauri::command]
pub async fn inspect_ai_client(
    app: tauri::AppHandle,
    client: Client,
) -> Result<Inspection, InspectionError> {
    let context = Context::production(&app)?;
    tauri::async_runtime::spawn_blocking(move || inspect(&context, client))
        .await
        .map_err(|_| InspectionError::unavailable())
}
#[tauri::command]
pub async fn preview_ai_client_configuration(
    app: tauri::AppHandle,
    client: Client,
) -> Result<ConfigurationPreview, InspectionError> {
    let context = Context::production(&app)?;
    tauri::async_runtime::spawn_blocking(move || preview(&context, client))
        .await
        .map_err(|_| InspectionError::unavailable())
}
#[cfg(test)]
mod tests;

#[tauri::command]
pub async fn prepare_client_configuration(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    service: tauri::State<'_, std::sync::Arc<ConsentService>>,
    client: Client,
) -> Result<ConsentPreview, ConsentError> {
    #[cfg(target_os = "macos")]
    {
        let context = Context::production(&app)
            .map_err(|_| ConsentError::new(ConsentCode::UnsupportedConfiguration))?;
        let service = service.inner().clone();
        let owner = window.label().to_owned();
        return tauri::async_runtime::spawn_blocking(move || {
            service.prepare(context, client, &owner)
        })
        .await
        .map_err(|_| ConsentError::new(ConsentCode::OperationFailed))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, window, service, client);
        Err(ConsentError::new(ConsentCode::UnsupportedPlatform))
    }
}
#[tauri::command]
pub async fn approve_client_configuration(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    service: tauri::State<'_, std::sync::Arc<ConsentService>>,
    plan_id: String,
) -> Result<ApprovalReceipt, ConsentError> {
    #[cfg(target_os = "macos")]
    {
        let context = Context::production(&app)
            .map_err(|_| ConsentError::new(ConsentCode::UnsupportedConfiguration))?;
        let service = service.inner().clone();
        let owner = window.label().to_owned();
        return tauri::async_runtime::spawn_blocking(move || {
            service.approve(context, &plan_id, &owner)
        })
        .await
        .map_err(|_| ConsentError::new(ConsentCode::OperationFailed))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, window, service, plan_id);
        Err(ConsentError::new(ConsentCode::UnsupportedPlatform))
    }
}
#[tauri::command]
pub fn reject_client_configuration_plan(
    window: tauri::WebviewWindow,
    service: tauri::State<'_, std::sync::Arc<ConsentService>>,
    plan_id: String,
) -> Result<ApprovalReceipt, ConsentError> {
    #[cfg(target_os = "macos")]
    {
        return service.reject(&plan_id, window.label());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, service, plan_id);
        Err(ConsentError::new(ConsentCode::UnsupportedPlatform))
    }
}
#[tauri::command]
pub async fn get_client_configuration_recovery(
    app: tauri::AppHandle,
) -> Result<RecoveryUsage, ConsentError> {
    #[cfg(target_os = "macos")]
    {
        let context = Context::production(&app)
            .map_err(|_| ConsentError::new(ConsentCode::UnsupportedConfiguration))?;
        return tauri::async_runtime::spawn_blocking(move || {
            let data = context
                .managed
                .root
                .parent()
                .ok_or_else(|| ConsentError::new(ConsentCode::UnsupportedConfiguration))?;
            write_engine::Engine::preview_only(&context.home, data)
                .map(|e| e.recovery_usage())
                .map_err(|_| ConsentError::new(ConsentCode::UnsupportedConfiguration))
        })
        .await
        .map_err(|_| ConsentError::new(ConsentCode::OperationFailed))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err(ConsentError::new(ConsentCode::UnsupportedPlatform))
    }
}
