#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

const HOME_URL: &str = "https://music.youtube.com";
// User-Agent de Chrome: evita que Google bloquee el inicio de sesión por "navegador no seguro".
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

fn show(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        let _ = w.eval("window.__lm&&window.__lm.setHidden(false)");
    }
}

fn hide(w: &WebviewWindow) {
    let _ = w.eval("window.__lm&&window.__lm.setHidden(true)");
    let _ = w.hide();
}

fn js(app: &AppHandle, code: &str) {
    if let Some(w) = main_window(app) {
        let _ = w.eval(code);
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .setup(|app| {
            #[allow(unused_mut)]
            let mut builder = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(HOME_URL.parse().unwrap()),
            )
            .title("Lite Music")
            .inner_size(1100.0, 720.0)
            .min_inner_size(420.0, 500.0)
            .user_agent(UA)
            .initialization_script(include_str!("inject.js"));

            // Menos procesos y sin servicios de red/actualización de fondo de WebView2.
            #[cfg(windows)]
            {
                builder = builder.additional_browser_args(
                    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate \
                     --disable-background-networking --disable-component-update \
                     --disable-sync --renderer-process-limit=1 --enable-low-end-device-mode",
                );
            }

            let window = builder.build()?;

            // Cerrar = ocultar a la bandeja; la música sigue sonando.
            let w = window.clone();
            window.on_window_event(move |e| {
                if let WindowEvent::CloseRequested { api, .. } = e {
                    api.prevent_close();
                    hide(&w);
                }
            });

            // Bandeja del sistema
            let m_show = MenuItem::with_id(app, "show", "Mostrar", true, None::<&str>)?;
            let m_pp = MenuItem::with_id(app, "pp", "Reproducir / Pausar", true, None::<&str>)?;
            let m_next = MenuItem::with_id(app, "next", "Siguiente", true, None::<&str>)?;
            let m_prev = MenuItem::with_id(app, "prev", "Anterior", true, None::<&str>)?;
            let m_quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&m_show, &sep, &m_pp, &m_prev, &m_next, &sep, &m_quit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Lite Music")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "show" => show(app),
                    "pp" => js(app, "window.__lm&&window.__lm.playPause()"),
                    "next" => js(app, "window.__lm&&window.__lm.next()"),
                    "prev" => js(app, "window.__lm&&window.__lm.prev()"),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        show(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al iniciar Lite Music");
}
