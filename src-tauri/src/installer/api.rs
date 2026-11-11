//! Fixed-purpose Tauri API: callers cannot choose paths, commands or versions.
pub use super::api_models::*;
pub use super::api_service::Service;
use std::sync::Arc;
use tauri::{Emitter, EventTarget};

fn start(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    service: &Arc<Service>,
    component: Component,
) -> Result<StartReceipt, InstallationError> {
    if !super::api_backend::supported() {
        return Err(InstallationError::new(ErrorCode::UnsupportedPlatform));
    }
    let paths = super::resolve_installer_paths(&app)
        .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
    let id =
        super::attempt::new_id().map_err(|_| InstallationError::new(ErrorCode::OperationFailed))?;
    let owner = window.label().to_owned();
    let target = owner.clone();
    let sink = Arc::new(move |event: InstallationEvent| {
        // Delivery failure does not cancel installation; the last event is queryable.
        let _ = app.emit_to(
            EventTarget::webview_window(&target),
            super::progress::EVENT_NAME,
            event,
        );
    });
    let operation = service.reserve(component, owner, id, sink)?;
    let receipt = StartReceipt {
        attempt_id: operation.progress.id.clone(),
        component,
        state: OperationState::Installing,
        target_version: component.version().into(),
    };
    let service = service.clone();
    tauri::async_runtime::spawn(async move {
        service.execute(operation, paths).await;
    });
    Ok(receipt)
}
#[tauri::command]
pub fn install_managed_node(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    service: tauri::State<'_, Arc<Service>>,
) -> Result<StartReceipt, InstallationError> {
    start(app, window, &service, Component::Node)
}
#[tauri::command]
pub fn install_managed_mcp(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    service: tauri::State<'_, Arc<Service>>,
) -> Result<StartReceipt, InstallationError> {
    start(app, window, &service, Component::Mcp)
}
#[tauri::command]
pub fn cancel_managed_installation(
    window: tauri::WebviewWindow,
    service: tauri::State<'_, Arc<Service>>,
    attempt_id: String,
) -> Result<CancellationReceipt, InstallationError> {
    service.cancel(window.label(), &attempt_id)
}
#[tauri::command]
pub async fn get_managed_installation_state(
    app: tauri::AppHandle,
    service: tauri::State<'_, Arc<Service>>,
) -> Result<ManagedInstallationReport, InstallationError> {
    let paths = super::resolve_installer_paths(&app)
        .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))?;
    service
        .inspect(|| {
            let paths = paths.clone();
            async move {
                super::deadline::run(move || async move { Ok(super::api_backend::inspect(&paths)) })
                    .await
                    .map_err(|_| InstallationError::new(ErrorCode::StateUnavailable))
            }
        })
        .await
}
