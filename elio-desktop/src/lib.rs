use std::fs;
use tauri::Manager;
use tauri::image::Image;

fn make_tray_icon() -> Image<'static> {
    let s: u32 = 32;
    let mut rgba = vec![0u8; (s * s * 4) as usize];
    let c = s as f32 / 2.0;
    let r = s as f32 / 2.0 - 1.0;
    for y in 0..s {
        for x in 0..s {
            let dx = x as f32 + 0.5 - c;
            let dy = y as f32 + 0.5 - c;
            let d = (dx * dx + dy * dy).sqrt();
            if d <= r {
                let i = ((y * s + x) * 4) as usize;
                rgba[i] = 46; rgba[i + 1] = 204; rgba[i + 2] = 113; rgba[i + 3] = 255;
            }
        }
    }
    Image::new_owned(rgba, s, s)
}

#[tauri::command]
fn disable_context_menu(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::webview::PlatformWebview;
        window.with_webview(|pw: PlatformWebview| {
            let controller = pw.controller();
            unsafe {
                let _ = controller.CoreWebView2()
                    .and_then(|core| core.Settings())
                    .and_then(|settings| settings.SetAreDefaultContextMenusEnabled(false));
            }
        }).map_err(|e| format!("{e:?}"))
    }
    #[cfg(not(target_os = "windows"))]
    Ok(())
}

#[tauri::command]
fn set_window_size(window: tauri::WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    use tauri::Size;
    window.set_size(Size::Logical((width, height).into())).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, settings: String) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("settings.json"), &settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_settings(app: tauri::AppHandle) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    match fs::read_to_string(dir.join("settings.json")) {
        Ok(s) => Ok(s),
        Err(_) => Ok("{}".to_string()),
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![disable_context_menu, set_window_size, save_settings, load_settings])
        .setup(|app| {
            let _tray = tauri::tray::TrayIconBuilder::new()
                .icon(make_tray_icon())
                .tooltip("Elio — 左键显示窗口")
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("启动 Elio 桌面失败");
}
