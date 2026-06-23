pub mod core;

use tauri::Manager;

#[tauri::command]
async fn core_status(
    state: tauri::State<'_, core::SharedState>,
) -> Result<core::GatewayState, String> {
    state
        .database
        .gateway_state()
        .map_err(|err| err.to_string())
}

fn apply_tray_proxy_action(state: &core::SharedState, action: &str) -> Result<bool, String> {
    match action {
        "proxy-start" => {
            state
                .database
                .set_proxy_enabled(true)
                .map_err(|err| err.to_string())?;
            state
                .database
                .add_log("info", "Gateway", "Proxy started from macOS tray")
                .map_err(|err| err.to_string())?;
            Ok(true)
        }
        "proxy-stop" => {
            state
                .database
                .set_proxy_enabled(false)
                .map_err(|err| err.to_string())?;
            state
                .database
                .add_log("warn", "Gateway", "Proxy stopped from macOS tray")
                .map_err(|err| err.to_string())?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub fn run() {
    let runtime = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let state = runtime
        .block_on(core::SharedState::new())
        .expect("failed to initialize codexswitch core");

    let server_state = state.clone();
    runtime.spawn(async move {
        if let Err(err) = core::serve(server_state, "127.0.0.1:3900").await {
            eprintln!("codexswitch local core failed: {err}");
        }
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![core_status])
        .setup(|app| {
            let show_item = tauri::menu::MenuItem::with_id(
                app,
                "show",
                "Open codexswitch",
                true,
                None::<&str>,
            )?;
            let status_item = tauri::menu::MenuItem::with_id(
                app,
                "proxy-status",
                "Proxy status: stopped",
                false,
                None::<&str>,
            )?;
            let start_item = tauri::menu::MenuItem::with_id(
                app,
                "proxy-start",
                "Start proxy",
                true,
                None::<&str>,
            )?;
            let stop_item = tauri::menu::MenuItem::with_id(
                app,
                "proxy-stop",
                "Stop proxy",
                true,
                None::<&str>,
            )?;
            let quit_item =
                tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(
                app,
                &[
                    &show_item,
                    &status_item,
                    &start_item,
                    &stop_item,
                    &quit_item,
                ],
            )?;
            let tray_status_item = status_item.clone();
            let _tray = tauri::tray::TrayIconBuilder::with_id("main")
                .tooltip("codexswitch")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.0.as_str() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "proxy-start" => {
                        if let Some(state) = app.try_state::<core::SharedState>() {
                            let _ = apply_tray_proxy_action(&state, "proxy-start");
                            let _ = tray_status_item.set_text("Proxy status: running");
                        }
                    }
                    "proxy-stop" => {
                        if let Some(state) = app.try_state::<core::SharedState>() {
                            let _ = apply_tray_proxy_action(&state, "proxy-stop");
                            let _ = tray_status_item.set_text("Proxy status: stopped");
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running codexswitch");

    drop(runtime);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_proxy_actions_update_gateway_state() {
        let state = core::SharedState::memory_for_test().unwrap();
        assert_eq!(state.database.gateway_state().unwrap().status, "stopped");

        assert!(apply_tray_proxy_action(&state, "proxy-start").unwrap());
        assert_eq!(state.database.gateway_state().unwrap().status, "running");

        assert!(apply_tray_proxy_action(&state, "proxy-stop").unwrap());
        assert_eq!(state.database.gateway_state().unwrap().status, "stopped");

        assert!(!apply_tray_proxy_action(&state, "unknown").unwrap());
    }
}
