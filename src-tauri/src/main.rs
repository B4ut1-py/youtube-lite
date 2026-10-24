#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    LogicalSize,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

const HOME_URL: &str = "https://music.youtube.com";
// User-Agent de Chrome: evita que Google bloquee el inicio de sesión por "navegador no seguro".
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

// Los dos webviews (YouTube y flotante) comparten entorno WebView2: los argumentos deben ser idénticos.
#[cfg(windows)]
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate \
     --disable-background-networking --disable-component-update --disable-gpu \
     --disable-sync --renderer-process-limit=1 --enable-low-end-device-mode";

#[derive(Clone, Default, Serialize, Deserialize)]
struct PlayerState {
    title: String,
    artist: String,
    playing: bool,
    pos: f64,
    dur: f64,
    #[serde(default)]
    skin: String,
}

struct AppState {
    scale: Mutex<f64>,
    player: Mutex<PlayerState>,
    float_visible: Mutex<bool>,
}

fn skin_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("skin.txt"))
}

fn load_skin(app: &AppHandle) -> String {
    skin_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| s == "dog" || s == "cat")
        .unwrap_or_else(|| "dog".into())
}

fn set_skin(app: &AppHandle, skin: &str) {
    app.state::<AppState>().player.lock().unwrap().skin = skin.into();
    if let Some(p) = skin_path(app) {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(p, skin);
    }
}

const FLOAT_W: f64 = 340.0;
const FLOAT_H: f64 = 350.0;

fn scale_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("size.txt"))
}

fn load_scale(app: &AppHandle) -> f64 {
    scale_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|v| v.clamp(0.4, 1.2))
        .unwrap_or(1.0)
}

// Cambia el tamaño del flotante: la ventana y su contenido (zoom) escalan juntos.
fn set_float_scale(app: &AppHandle, scale: f64) {
    let scale = scale.clamp(0.4, 1.2);
    *app.state::<AppState>().scale.lock().unwrap() = scale;
    if let Some(w) = app.get_webview_window("float") {
        let _ = w.set_size(LogicalSize::new(FLOAT_W * scale, FLOAT_H * scale));
        let _ = w.set_zoom(scale);
    }
    if let Some(p) = scale_path(app) {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(p, format!("{scale:.2}"));
    }
}

#[tauri::command]
fn scale_float(app: AppHandle, delta: f64) {
    let cur = *app.state::<AppState>().scale.lock().unwrap();
    set_float_scale(&app, cur + delta);
}

fn toggle_float(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("float") {
        let visible = w.is_visible().unwrap_or(true);
        let _ = if visible { w.hide() } else { w.show() };
        *app.state::<AppState>().float_visible.lock().unwrap() = !visible;
    }
}

// La página de YouTube Music informa de la canción; devuelve si el flotante está visible
// (la página usa eso para consultar con menos frecuencia cuando no hace falta).
#[tauri::command]
fn report_state(state: tauri::State<AppState>, title: String, artist: String, playing: bool, pos: f64, dur: f64) -> bool {
    let mut p = state.player.lock().unwrap();
    p.title = title;
    p.artist = artist;
    p.playing = playing;
    p.pos = pos;
    p.dur = dur;
    *state.float_visible.lock().unwrap()
}

#[tauri::command]
fn get_state(state: tauri::State<AppState>) -> PlayerState {
    state.player.lock().unwrap().clone()
}

#[tauri::command]
fn control(app: AppHandle, action: String, value: Option<f64>) {
    let code = match action.as_str() {
        "playpause" => "window.__lm&&window.__lm.playPause()".to_string(),
        "next" => "window.__lm&&window.__lm.next()".to_string(),
        "prev" => "window.__lm&&window.__lm.prev()".to_string(),
        "seek" => format!("window.__lm&&window.__lm.seek({})", value.unwrap_or(0.0).clamp(0.0, 1.0)),
        _ => return,
    };
    js(&app, &code);
}

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
        .invoke_handler(tauri::generate_handler![report_state, get_state, control, scale_float])
        .manage(AppState { scale: Mutex::new(1.0), player: Mutex::new(PlayerState::default()), float_visible: Mutex::new(true) })
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
                builder = builder.additional_browser_args(BROWSER_ARGS);
            }

            let window = builder.build()?;

            // Reproductor flotante: sin marco, sin fondo, siempre encima.
            #[allow(unused_mut)]
            let mut fb = WebviewWindowBuilder::new(app, "float", WebviewUrl::App("float.html".into()))
                .title("Lite Music Float")
                .inner_size(340.0, 350.0)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false);
            #[cfg(windows)]
            {
                fb = fb.additional_browser_args(BROWSER_ARGS);
            }
            fb.build()?;
            set_float_scale(app.handle(), load_scale(app.handle()));
            set_skin(app.handle(), &load_skin(app.handle()));

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
            let m_float = MenuItem::with_id(app, "float", "Mostrar / ocultar flotante", true, None::<&str>)?;
            let m_dog = MenuItem::with_id(app, "dog", "Flotante: perro", true, None::<&str>)?;
            let m_cat = MenuItem::with_id(app, "cat", "Flotante: gato", true, None::<&str>)?;
            let s50 = MenuItem::with_id(app, "sz50", "Muy pequeño (50%)", true, None::<&str>)?;
            let s70 = MenuItem::with_id(app, "sz70", "Pequeño (70%)", true, None::<&str>)?;
            let s100 = MenuItem::with_id(app, "sz100", "Normal (100%)", true, None::<&str>)?;
            let m_size = Submenu::with_items(app, "Tamaño del flotante", true, &[&s50, &s70, &s100])?;
            let m_quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&m_show, &sep, &m_pp, &m_prev, &m_next, &sep, &m_float, &m_dog, &m_cat, &sep, &m_size, &sep, &m_quit])?;

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
                    "float" => toggle_float(app),
                    "sz50" => set_float_scale(app, 0.5),
                    "sz70" => set_float_scale(app, 0.7),
                    "sz100" => set_float_scale(app, 1.0),
                    "dog" => set_skin(app, "dog"),
                    "cat" => set_skin(app, "cat"),
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
