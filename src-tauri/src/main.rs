#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri_plugin_updater::UpdaterExt;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
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
    cover: String,
    vol: f64,
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
        .filter(|s| matches!(s.as_str(), "dog" | "cat" | "cover" | "none" | "trabajando" | "programando" | "tradeando" | "descansando" | "estudiando" | "festejando"))
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
const FLOAT_H: f64 = 385.0;

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
fn report_state(state: tauri::State<AppState>, title: String, artist: String, cover: String, vol: f64, playing: bool, pos: f64, dur: f64) -> bool {
    let mut p = state.player.lock().unwrap();
    p.title = title;
    p.artist = artist;
    p.cover = cover;
    p.vol = vol;
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
        "volume" => format!("window.__lm&&window.__lm.setVolume({})", value.unwrap_or(1.0).clamp(0.0, 1.0)),
        "seek" => format!("window.__lm&&window.__lm.seek({})", value.unwrap_or(0.0).clamp(0.0, 1.0)),
        _ => return,
    };
    js(&app, &code);
}

// Busca una versión nueva en GitHub; si hay, la descarga e instala (en Windows el instalador
// cierra la app, actualiza y la vuelve a abrir). "manual" solo cambia el aviso del tooltip.
async fn check_update(app: AppHandle, manual: bool) {
    let say = |msg: &str| {
        if let Some(t) = app.tray_by_id("main") {
            let _ = t.set_tooltip(Some(msg));
        }
    };
    let Ok(updater) = app.updater() else { return };
    match updater.check().await {
        Ok(Some(update)) => {
            say(&format!("Lite Music: instalando versión {}…", update.version));
            if update.download_and_install(|_, _| {}, || {}).await.is_ok() {
                #[cfg(not(windows))]
                app.restart();
            } else {
                say("Lite Music: no se pudo actualizar");
            }
        }
        Ok(None) => {
            if manual {
                say("Lite Music: ya tienes la última versión");
            }
        }
        Err(_) => {
            if manual {
                say("Lite Music: no se pudo buscar actualizaciones");
            }
        }
    }
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

async fn tokio_sleep(secs: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_secs(secs))).await;
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![report_state, get_state, control, scale_float])
        .manage(AppState { scale: Mutex::new(1.0), player: Mutex::new(PlayerState::default()), float_visible: Mutex::new(true) })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
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
            // Arranque con Windows: empieza oculta en la bandeja.
            if std::env::args().any(|a| a == "--minimized") {
                hide(&window);
            }

            // Reproductor flotante: sin marco, sin fondo, siempre encima.
            #[allow(unused_mut)]
            let mut fb = WebviewWindowBuilder::new(app, "float", WebviewUrl::App("float.html".into()))
                .title("Lite Music Float")
                .inner_size(340.0, 385.0)
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
            let m_dog = MenuItem::with_id(app, "dog", "Perro", true, None::<&str>)?;
            let m_cat = MenuItem::with_id(app, "cat", "Gato", true, None::<&str>)?;
            let m_cover = MenuItem::with_id(app, "cover", "Carátula", true, None::<&str>)?;
            let m_none = MenuItem::with_id(app, "none", "Sin imagen (solo controles)", true, None::<&str>)?;
            let m_trab = MenuItem::with_id(app, "trabajando", "Mood: trabajando", true, None::<&str>)?;
            let m_prog = MenuItem::with_id(app, "programando", "Mood: programando", true, None::<&str>)?;
            let m_trad = MenuItem::with_id(app, "tradeando", "Mood: tradeando", true, None::<&str>)?;
            let m_desc = MenuItem::with_id(app, "descansando", "Mood: descansando", true, None::<&str>)?;
            let m_estu = MenuItem::with_id(app, "estudiando", "Mood: estudiando", true, None::<&str>)?;
            let m_fest = MenuItem::with_id(app, "festejando", "Mood: festejando", true, None::<&str>)?;
            let m_skin = Submenu::with_items(app, "Aspecto del flotante", true, &[&m_dog, &m_cat, &m_cover, &m_none, &PredefinedMenuItem::separator(app)?, &m_trab, &m_prog, &m_trad, &m_desc, &m_estu, &m_fest])?;
            let m_auto = CheckMenuItem::with_id(app, "auto", "Iniciar con Windows", true, app.autolaunch().is_enabled().unwrap_or(false), None::<&str>)?;
            let m_auto_c = m_auto.clone();
            let s50 = MenuItem::with_id(app, "sz50", "Muy pequeño (50%)", true, None::<&str>)?;
            let s70 = MenuItem::with_id(app, "sz70", "Pequeño (70%)", true, None::<&str>)?;
            let s100 = MenuItem::with_id(app, "sz100", "Normal (100%)", true, None::<&str>)?;
            let m_size = Submenu::with_items(app, "Tamaño del flotante", true, &[&s50, &s70, &s100])?;
            let m_upd = MenuItem::with_id(app, "upd", "Buscar actualizaciones", true, None::<&str>)?;
            let m_quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&m_show, &sep, &m_pp, &m_prev, &m_next, &sep, &m_float, &m_skin, &m_size, &sep, &m_auto, &m_upd, &sep, &m_quit])?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Lite Music")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, ev| match ev.id.as_ref() {
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
                    "cover" => set_skin(app, "cover"),
                    "none" => set_skin(app, "none"),
                    id @ ("trabajando" | "programando" | "tradeando" | "descansando" | "estudiando" | "festejando") => set_skin(app, id),
                    "upd" => {
                        let a = app.clone();
                        tauri::async_runtime::spawn(check_update(a, true));
                    }
                    "auto" => {
                        let al = app.autolaunch();
                        let _ = if m_auto_c.is_checked().unwrap_or(false) { al.enable() } else { al.disable() };
                    }
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

            // Actualización automática al arrancar (tras unos segundos, para no competir con la carga).
            let a = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio_sleep(8).await;
                check_update(a, false).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al iniciar Lite Music");
}
