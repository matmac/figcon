mod acceptance;
mod client_integration;
mod environment;
mod installer;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let session =
        acceptance::Session::from_process().expect("Native acceptance environment rejected");
    let mut context = tauri::generate_context!();
    session
        .configure(context.config_mut())
        .expect("Native acceptance environment rejected");
    tauri::Builder::default()
        .manage(session)
        .manage(std::sync::Arc::new(installer::api::Service::default()))
        .manage(std::sync::Arc::new(
            client_integration::ConsentService::default(),
        ))
        .setup(|app| {
            client_integration::ConsentService::start_expiry_worker(
                app.state::<std::sync::Arc<client_integration::ConsentService>>()
                    .inner(),
            );
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            client_integration::prepare_client_configuration,
            client_integration::approve_client_configuration,
            client_integration::reject_client_configuration_plan,
            client_integration::get_client_configuration_recovery,
            client_integration::list_ai_clients,
            client_integration::inspect_ai_client,
            client_integration::preview_ai_client_configuration,
            installer::api::install_managed_node,
            installer::api::install_managed_mcp,
            installer::api::cancel_managed_installation,
            installer::api::get_managed_installation_state,
            environment::detect_environment,
            installer::status::get_installation_status,
            installer::discovery::discover_claude_code_mcp
        ])
        .run(context)
        .expect("error while running tauri application");
}
