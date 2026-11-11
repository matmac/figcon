mod environment;
mod installer;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(std::sync::Arc::new(installer::api::Service::default()))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            installer::api::install_managed_node,
            installer::api::install_managed_mcp,
            installer::api::cancel_managed_installation,
            installer::api::get_managed_installation_state,
            environment::detect_environment,
            installer::status::get_installation_status,
            installer::discovery::discover_claude_code_mcp
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
