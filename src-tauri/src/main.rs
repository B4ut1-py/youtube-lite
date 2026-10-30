#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, sync::Mutex, time::{Duration, Instant}};

use serde::{Deserialize, Serialize};
use tauri_plugin_updater::UpdaterExt;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    LogicalSize, PhysicalPosition,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

const YTM_URL: &str = "https://music.youtube.com";
const SPOTIFY_URL: &str = "https://open.spotify.com";

fn source_url(source: &str) -> &'static str {
    if source == "spotify" { SPOTIFY_URL } else { YTM_URL }
}
// User-Agent de Chrome: evita que Google bloquee el inicio de sesión por "navegador no seguro".
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

// Los dos webviews (YouTube y flotante) comparten entorno WebView2: los argumentos deben ser idénticos.
#[cfg(windows)]
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate \
     --disable-gpu \
     --disable-sync --renderer-process-limit=1 --enable-low-end-device-mode";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    skin: String,
    scale: f64,
    autohide: bool,
    float_opacity: f64,
    float_on_top: bool,
    float_at_start: bool,
    blink: bool,
    reactions: bool,
    wave: bool,
    wave_min_s: u64,
    wave_max_s: u64,
    yawn: bool,
    yawn_idle_s: u64,
    mood_min_s: u64,
    mood_max_s: u64,
    pomo_focus_min: u64,
    pomo_rest_min: u64,
    close_to_tray: bool,
    auto_update: bool,
    idle_webview: bool,
    idle_hide_ui: bool,
    destroy_float: bool,
    suspend_idle: bool,
    suspend_after_s: u64,
    source: String,
    welcomed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            skin: "dog".into(), scale: 1.0, autohide: true, float_opacity: 1.0, float_on_top: true, float_at_start: true,
            blink: true, reactions: true, wave: true, wave_min_s: 25, wave_max_s: 55, yawn: true, yawn_idle_s: 90,
            mood_min_s: 15, mood_max_s: 30, pomo_focus_min: 25, pomo_rest_min: 5, close_to_tray: true, auto_update: true,
            idle_webview: true, idle_hide_ui: true, destroy_float: true, suspend_idle: true, suspend_after_s: 90, source: "ytm".into(), welcomed: false,
        }
    }
}

const SKINS: [&str; 10] = ["dog", "cat", "cover", "none", "trabajando", "programando", "tradeando", "descansando", "estudiando", "festejando"];

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
    pomo_phase: String,
    pomo_left: u64,
    cfg: Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase { Off, Focus, Rest }

struct Pomo {
    phase: Phase,
    end: Instant,
    focus_s: u64,
    rest_s: u64,
    prev_skin: String,
}

struct AppState {
    pomo: Mutex<Pomo>,
    user_float: Mutex<bool>,      // el usuario quiere ver el flotante
    auto_hidden: Mutex<bool>,     // oculto automáticamente (juego / pantalla completa)
    settings: Mutex<Settings>,
    pending_pos: Mutex<Option<(i32, i32)>>,
    player: Mutex<PlayerState>,
    float_visible: Mutex<bool>,
    last_report: Mutex<Instant>,   // última vez que la página de música dio señales de vida
}

const FLOAT_W: f64 = 340.0;
const FLOAT_H: f64 = 385.0;

fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn save_settings(app: &AppHandle) {
    let json = serde_json::to_string_pretty(&*app.state::<AppState>().settings.lock().unwrap()).unwrap_or_default();
    if let Some(p) = settings_path(app) {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(p, json);
    }
}

// Lee settings.json; si no existe (versiones anteriores) toma lo guardado en los archivos sueltos.
fn load_settings(app: &AppHandle) -> Settings {
    if let Some(s) = settings_path(app).and_then(|p| fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str::<Settings>(&t).ok()) {
        return s;
    }
    let mut s = Settings::default();
    if let Some(v) = cfg_read(app, "skin.txt").filter(|v| SKINS.contains(&v.as_str())) { s.skin = v; }
    if let Some(v) = cfg_read(app, "size.txt").and_then(|v| v.parse::<f64>().ok()) { s.scale = v.clamp(0.4, 1.2); }
    if cfg_read(app, "autohide.txt").as_deref() == Some("off") { s.autohide = false; }
    s
}

// Cambia entre YouTube Music y Spotify: la misma ventana navega al otro sitio (nunca hay dos páginas cargadas).
fn set_source(app: &AppHandle, source: &str) {
    if source != "ytm" && source != "spotify" {
        return;
    }
    app.state::<AppState>().settings.lock().unwrap().source = source.into();
    save_settings(app);
    wake(app);
    if let Some(w) = main_window(app) {
        let _ = w.navigate(source_url(source).parse().unwrap());
        show(app);
    }
}

// Pantalla de bienvenida (primer inicio) y elegir servicio.
#[tauri::command]
fn choose_source(app: AppHandle, source: String) {
    app.state::<AppState>().settings.lock().unwrap().welcomed = true;
    set_source(&app, &source);
    if let Some(w) = app.get_webview_window("welcome") {
        let _ = w.close();
    }
}

// Abre en el navegador uno de los enlaces fijos del autor (nunca una dirección recibida de fuera).
#[tauri::command]
fn open_link(which: String) {
    let url = match which.as_str() {
        "github" => "https://github.com/B4ut1-py",
        "linkedin" => "https://www.linkedin.com/in/bautista-rubio/",
        _ => return,
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer.exe").arg(url).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn open_welcome(app: &AppHandle) {
    #[allow(unused_mut)]
    let mut b = WebviewWindowBuilder::new(app, "welcome", WebviewUrl::App("welcome.html".into()))
        .title("Lite Music")
        .inner_size(600.0, 600.0)
        .resizable(false)
        .maximizable(false)
        .center();
    #[cfg(windows)]
    {
        b = b.additional_browser_args(BROWSER_ARGS);
    }
    if let Ok(w) = b.build() {
        let a = app.clone();
        w.on_window_event(move |e| {
            // Cerrar sin elegir: no hay servicio que mostrar, se sale y se vuelve a preguntar la próxima vez.
            if let WindowEvent::CloseRequested { .. } = e {
                if !a.state::<AppState>().settings.lock().unwrap().welcomed {
                    a.exit(0);
                }
            }
        });
    }
}

fn set_skin(app: &AppHandle, skin: &str) {
    if !SKINS.contains(&skin) {
        return;
    }
    app.state::<AppState>().player.lock().unwrap().skin = skin.into();
    app.state::<AppState>().settings.lock().unwrap().skin = skin.into();
    save_settings(app);
}

// Cambia el tamaño del flotante: la ventana y su contenido (zoom) escalan juntos.
fn set_float_scale(app: &AppHandle, scale: f64) {
    let scale = scale.clamp(0.4, 1.2);
    app.state::<AppState>().settings.lock().unwrap().scale = scale;
    if let Some(w) = app.get_webview_window("float") {
        let _ = w.set_size(LogicalSize::new(FLOAT_W * scale, FLOAT_H * scale));
        let _ = w.set_zoom(scale);
    }
    save_settings(app);
}

#[tauri::command]
fn scale_float(app: AppHandle, delta: f64) {
    let cur = app.state::<AppState>().settings.lock().unwrap().scale;
    set_float_scale(&app, cur + delta);
}

fn cfg_path(app: &AppHandle, name: &str) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(name))
}

fn cfg_read(app: &AppHandle, name: &str) -> Option<String> {
    cfg_path(app, name).and_then(|p| fs::read_to_string(p).ok()).map(|s| s.trim().to_string())
}

fn cfg_write(app: &AppHandle, name: &str, value: &str) {
    if let Some(p) = cfg_path(app, name) {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(p, value);
    }
}

// Crea el flotante (sin marco, sin fondo, siempre encima) con el tamaño, posición y opciones guardados.
fn create_float(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let cfg = app.state::<AppState>().settings.lock().unwrap().clone();
    #[allow(unused_mut)]
    let mut fb = WebviewWindowBuilder::new(app, "float", WebviewUrl::App("float.html".into()))
        .title("Lite Music Float")
        .inner_size(FLOAT_W * cfg.scale, FLOAT_H * cfg.scale)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(cfg.float_on_top)
        .skip_taskbar(true)
        .resizable(false);
    #[cfg(windows)]
    {
        fb = fb.additional_browser_args(BROWSER_ARGS);
    }
    let w = fb.build()?;
    let _ = w.set_zoom(cfg.scale);
    restore_float_pos(app, &w);
    let ah = app.clone();
    w.on_window_event(move |e| {
        if let WindowEvent::Moved(p) = e {
            *ah.state::<AppState>().pending_pos.lock().unwrap() = Some((p.x, p.y));
        }
    });
    Ok(w)
}

static FLOAT_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn float_wanted(app: &AppHandle) -> bool {
    let st = app.state::<AppState>();
    let want = *st.user_float.lock().unwrap() && !*st.auto_hidden.lock().unwrap();
    want
}

// ¿Coincide lo que se ve con lo que se quiere ver? (el latido de cada segundo lo corrige si no)
fn float_in_sync(app: &AppHandle) -> bool {
    let want = float_wanted(app);
    let destroy = app.state::<AppState>().settings.lock().unwrap().destroy_float;
    match app.get_webview_window("float") {
        Some(w) => want == w.is_visible().unwrap_or(true) || (!want && false),
        None => !want && destroy,
    }
}

// Visible = lo quiere el usuario Y no está oculto por un juego / pantalla completa.
// Con el ahorro activado, oculto = la ventana se destruye (libera su proceso) y se recrea al mostrarla.
fn apply_float_visibility(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    let want = float_wanted(app);
    *app.state::<AppState>().float_visible.lock().unwrap() = want;
    if FLOAT_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let a = app.clone();
    // Crear o destruir ventanas desde un hilo aparte evita bloqueos en Windows.
    std::thread::spawn(move || {
        let destroy = a.state::<AppState>().settings.lock().unwrap().destroy_float;
        match (want, a.get_webview_window("float")) {
            (true, Some(w)) => {
                if !w.is_visible().unwrap_or(true) {
                    let _ = w.show();
                }
            }
            (true, None) => {
                let _ = create_float(&a);
            }
            (false, Some(w)) => {
                if destroy {
                    if let Ok(p) = w.outer_position() {
                        cfg_write(&a, "pos.txt", &format!("{},{}", p.x, p.y));
                    }
                    let _ = w.destroy();
                } else {
                    let _ = w.hide();
                }
            }
            (false, None) => {}
        }
        FLOAT_BUSY.store(false, Ordering::SeqCst);
    });
}

fn toggle_float(app: &AppHandle) {
    {
        let st = app.state::<AppState>();
        let mut u = st.user_float.lock().unwrap();
        *u = !*u;
    }
    apply_float_visibility(app);
}

// Restaura la posición guardada del flotante si cae dentro de algún monitor conectado.
fn restore_float_pos(app: &AppHandle, w: &WebviewWindow) {
    let Some(txt) = cfg_read(app, "pos.txt") else { return };
    let mut it = txt.split(',').filter_map(|v| v.trim().parse::<i32>().ok());
    let (Some(x), Some(y)) = (it.next(), it.next()) else { return };
    let inside = app.available_monitors().unwrap_or_default().iter().any(|m| {
        let (p, z) = (m.position(), m.size());
        x + 40 >= p.x && x + 40 < p.x + z.width as i32 && y + 40 >= p.y && y + 40 < p.y + z.height as i32
    });
    if inside {
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
}

// ¿Hay un juego o una aplicación a pantalla completa en primer plano? (solo Windows)
#[cfg(windows)]
fn fullscreen_busy() -> bool {
    #[link(name = "shell32")]
    extern "system" {
        fn SHQueryUserNotificationState(state: *mut i32) -> i32;
    }
    let mut state: i32 = 0;
    // 2 = pantalla completa / presentación, 3 = juego Direct3D a pantalla completa, 4 = modo presentación
    unsafe { SHQueryUserNotificationState(&mut state) == 0 && matches!(state, 2 | 3 | 4) }
}
#[cfg(not(windows))]
fn fullscreen_busy() -> bool {
    false
}

// --- Pomodoro: estudio -> descanso -> estudio... hasta que lo detengas ---
fn set_skin_temp(app: &AppHandle, skin: &str) {
    app.state::<AppState>().player.lock().unwrap().skin = skin.into();
}

fn start_pomo(app: &AppHandle, focus_min: u64, rest_min: u64) {
    let prev = {
        let st = app.state::<AppState>();
        let mut p = st.pomo.lock().unwrap();
        if p.phase == Phase::Off {
            p.prev_skin = st.player.lock().unwrap().skin.clone();
        }
        p.phase = Phase::Focus;
        p.focus_s = focus_min * 60;
        p.rest_s = rest_min * 60;
        p.end = Instant::now() + Duration::from_secs(p.focus_s);
        p.prev_skin.clone()
    };
    let _ = prev;
    set_skin_temp(app, "estudiando");
    js(app, "window.__lm&&window.__lm.play()");
}

fn stop_pomo(app: &AppHandle) {
    let prev = {
        let st = app.state::<AppState>();
        let mut p = st.pomo.lock().unwrap();
        if p.phase == Phase::Off {
            return;
        }
        p.phase = Phase::Off;
        p.prev_skin.clone()
    };
    if !prev.is_empty() {
        set_skin_temp(app, &prev);
    }
    if let Some(t) = app.tray_by_id("main") {
        let _ = t.set_tooltip(Some("Lite Music"));
    }
}

fn pomo_tick(app: &AppHandle) {
    let (changed, phase, left) = {
        let st = app.state::<AppState>();
        let mut p = st.pomo.lock().unwrap();
        if p.phase == Phase::Off {
            return;
        }
        let now = Instant::now();
        let mut changed = None;
        if now >= p.end {
            if p.phase == Phase::Focus {
                p.phase = Phase::Rest;
                p.end = now + Duration::from_secs(p.rest_s);
            } else {
                p.phase = Phase::Focus;
                p.end = now + Duration::from_secs(p.focus_s);
            }
            changed = Some(p.phase);
        }
        (changed, p.phase, p.end.saturating_duration_since(now).as_secs())
    };
    match changed {
        Some(Phase::Rest) => {
            set_skin_temp(app, "descansando");
            js(app, "window.__lm&&window.__lm.pause()");
        }
        Some(Phase::Focus) => {
            set_skin_temp(app, "estudiando");
            js(app, "window.__lm&&window.__lm.play()");
        }
        _ => {}
    }
    if let Some(t) = app.tray_by_id("main") {
        let name = if phase == Phase::Focus { "Estudio" } else { "Descanso" };
        let _ = t.set_tooltip(Some(&format!("Lite Music — {name} {}:{:02}", left / 60, left % 60)));
    }
}

// Un solo hilo con un latido por segundo: pomodoro, ocultarse en pantalla completa y guardar la posición.
fn start_ticker(app: AppHandle) {
    std::thread::spawn(move || {
    let mut last_title = String::new();
    let mut trim_at: Option<Instant> = None;
    let mut last_trim = Instant::now();
    let mut quiet_since = Instant::now();
    let mut last_reload = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        // Ventana oculta: tras cambiar de canción (o cada 3 min) la página vuelve a crecer; se le pide de nuevo bajar la memoria.
        if let Some(w) = main_window(&app) {
            let st = app.state::<AppState>();
            let enabled = st.settings.lock().unwrap().idle_webview;
            let hidden = !w.is_visible().unwrap_or(true);
            let (susp, after) = {
                let c = st.settings.lock().unwrap();
                (c.suspend_idle && c.idle_webview, c.suspend_after_s)
            };
            let playing = st.player.lock().unwrap().playing;
            if playing || !hidden {
                quiet_since = Instant::now();
            }
            let suspended = SUSPENDED.load(std::sync::atomic::Ordering::SeqCst);
            // Vigilante: si la página lleva un rato sin dar señales (se cortó la conexión y quedó la pantalla de error,
            // o la página murió en segundo plano), se recarga sola. No toca la página de inicio de sesión ni la suspendida.
            let (welcomed, source) = {
                let c = st.settings.lock().unwrap();
                (c.welcomed, c.source.clone())
            };
            let on_service = w.url().map_or(false, |u| {
                matches!(u.host_str(), Some("music.youtube.com") | Some("open.spotify.com")) || !matches!(u.scheme(), "http" | "https")
            });
            if welcomed && !suspended && on_service
                && st.last_report.lock().unwrap().elapsed() > Duration::from_secs(150)
                && last_reload.elapsed() > Duration::from_secs(60)
            {
                let _ = w.navigate(source_url(&source).parse().unwrap());
                last_reload = Instant::now();
                *st.last_report.lock().unwrap() = Instant::now();
            }
            // Sin música y con la ventana oculta: se congela la página hasta que vuelva a hacer falta.
            if susp && hidden && !playing && !suspended && quiet_since.elapsed() >= Duration::from_secs(after) {
                set_webview_suspended(&w, true);
                SUSPENDED.store(true, std::sync::atomic::Ordering::SeqCst);
            } else if suspended && (!susp || !hidden) {
                wake(&app);
            }
            if enabled && hidden && !suspended {
                let title = st.player.lock().unwrap().title.clone();
                if title != last_title {
                    last_title = title;
                    trim_at = Some(Instant::now() + Duration::from_secs(10));
                }
                let due = trim_at.map_or(false, |t| Instant::now() >= t) || last_trim.elapsed() > Duration::from_secs(180);
                if due {
                    set_webview_memory(&w, false);
                    std::thread::sleep(Duration::from_millis(300));
                    set_webview_memory(&w, true);
                    trim_at = None;
                    last_trim = Instant::now();
                }
            } else {
                last_trim = Instant::now();
            }
        }
        pomo_tick(&app);
        let st = app.state::<AppState>();
        let want = st.settings.lock().unwrap().autohide && fullscreen_busy();
        let changed = {
            let mut a = st.auto_hidden.lock().unwrap();
            let c = *a != want;
            *a = want;
            c
        };
        if changed || !float_in_sync(&app) {
            apply_float_visibility(&app);
        }
        let pos = st.pending_pos.lock().unwrap().take();
        if let Some((x, y)) = pos {
            cfg_write(&app, "pos.txt", &format!("{x},{y}"));
        }
    }
    });
}

// La página de YouTube Music informa de la canción; devuelve si el flotante está visible
// (la página usa eso para consultar con menos frecuencia cuando no hace falta).
#[tauri::command]
fn report_state(state: tauri::State<AppState>, title: String, artist: String, cover: String, vol: f64, playing: bool, pos: f64, dur: f64) -> bool {
    *state.last_report.lock().unwrap() = Instant::now();
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
    let mut p = state.player.lock().unwrap().clone();
    let pm = state.pomo.lock().unwrap();
    p.pomo_phase = match pm.phase { Phase::Off => "off", Phase::Focus => "focus", Phase::Rest => "rest" }.into();
    p.cfg = state.settings.lock().unwrap().clone();
    p.pomo_left = if pm.phase == Phase::Off { 0 } else { pm.end.saturating_duration_since(Instant::now()).as_secs() };
    p
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

// Elementos de menú que avisan de una versión nueva (barra superior de la ventana y bandeja).
struct UpdUi {
    menu: Menu<tauri::Wry>,
    bar_item: MenuItem<tauri::Wry>,
    tray_item: MenuItem<tauri::Wry>,
    shown: bool,
    version: Option<String>,
}
static UPD: std::sync::OnceLock<Mutex<UpdUi>> = std::sync::OnceLock::new();

fn set_update_available(app: &AppHandle, version: Option<String>) {
    let Some(u) = UPD.get() else { return };
    let mut u = u.lock().unwrap();
    match &version {
        Some(v) => {
            let _ = u.bar_item.set_text(format!("⬆  Nueva versión {v} disponible: instalar"));
            let _ = u.tray_item.set_text(format!("⬆ Instalar versión {v}"));
            if !u.shown {
                let item = u.bar_item.clone();
                if u.menu.append(&item).is_ok() {
                    u.shown = true;
                }
            }
            if let Some(t) = app.tray_by_id("main") {
                let _ = t.set_tooltip(Some(&format!("Lite Music: versión {v} disponible")));
            }
        }
        None => {
            if u.shown {
                let item = u.bar_item.clone();
                let _ = u.menu.remove(&item);
                u.shown = false;
            }
            let _ = u.tray_item.set_text("Buscar actualizaciones");
        }
    }
    u.version = version;
}

fn update_pending() -> bool {
    UPD.get().map_or(false, |u| u.lock().unwrap().version.is_some())
}

fn say(app: &AppHandle, msg: &str) {
    if let Some(t) = app.tray_by_id("main") {
        let _ = t.set_tooltip(Some(msg));
    }
}

// Busca una versión nueva en GitHub y AVISA en la ventana y en la bandeja. Nunca instala por su cuenta.
async fn check_update(app: AppHandle, manual: bool) {
    let Ok(updater) = app.updater() else { return };
    match updater.check().await {
        Ok(Some(update)) => {
            say(&app, &format!("Lite Music: versión {} disponible", update.version));
            set_update_available(&app, Some(update.version));
        }
        Ok(None) => {
            set_update_available(&app, None);
            if manual {
                say(&app, "Lite Music: ya tienes la última versión");
            }
        }
        Err(_) => {
            if manual {
                say(&app, "Lite Music: no se pudo buscar actualizaciones");
            }
        }
    }
}

// Descarga e instala la versión nueva, solo cuando el usuario lo elige (en Windows el instalador
// cierra la app, actualiza y la vuelve a abrir).
async fn install_update(app: AppHandle) {
    let Ok(updater) = app.updater() else { return };
    match updater.check().await {
        Ok(Some(update)) => {
            say(&app, &format!("Lite Music: instalando versión {}…", update.version));
            if update.download_and_install(|_, _| {}, || {}).await.is_ok() {
                #[cfg(not(windows))]
                app.restart();
            } else {
                say(&app, "Lite Music: no se pudo actualizar");
            }
        }
        _ => say(&app, "Lite Music: no se pudo actualizar"),
    }
}

#[derive(Serialize)]
struct SettingsView {
    settings: Settings,
    autostart: bool,
    version: String,
}

#[tauri::command]
fn get_settings(app: AppHandle) -> SettingsView {
    SettingsView {
        settings: app.state::<AppState>().settings.lock().unwrap().clone(),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        version: app.package_info().version.to_string(),
    }
}

// Cambia un ajuste y aplica su efecto al momento. El valor llega como JSON desde la ventana de configuración.
#[tauri::command]
fn set_setting(app: AppHandle, key: String, value: serde_json::Value) {
    let b = value.as_bool();
    let n = value.as_f64();
    {
        let st = app.state::<AppState>();
        let mut c = st.settings.lock().unwrap();
        match key.as_str() {
            "autohide" => if let Some(v) = b { c.autohide = v },
            "float_opacity" => if let Some(v) = n { c.float_opacity = v.clamp(0.3, 1.0) },
            "float_on_top" => if let Some(v) = b { c.float_on_top = v },
            "float_at_start" => if let Some(v) = b { c.float_at_start = v },
            "blink" => if let Some(v) = b { c.blink = v },
            "reactions" => if let Some(v) = b { c.reactions = v },
            "wave" => if let Some(v) = b { c.wave = v },
            "wave_min_s" => if let Some(v) = n { c.wave_min_s = (v as u64).clamp(5, 600); c.wave_max_s = c.wave_max_s.max(c.wave_min_s) },
            "wave_max_s" => if let Some(v) = n { c.wave_max_s = (v as u64).clamp(5, 900).max(c.wave_min_s) },
            "yawn" => if let Some(v) = b { c.yawn = v },
            "yawn_idle_s" => if let Some(v) = n { c.yawn_idle_s = (v as u64).clamp(20, 900) },
            "mood_min_s" => if let Some(v) = n { c.mood_min_s = (v as u64).clamp(5, 300); c.mood_max_s = c.mood_max_s.max(c.mood_min_s) },
            "mood_max_s" => if let Some(v) = n { c.mood_max_s = (v as u64).clamp(5, 600).max(c.mood_min_s) },
            "pomo_focus_min" => if let Some(v) = n { c.pomo_focus_min = (v as u64).clamp(1, 180) },
            "pomo_rest_min" => if let Some(v) = n { c.pomo_rest_min = (v as u64).clamp(1, 60) },
            "close_to_tray" => if let Some(v) = b { c.close_to_tray = v },
            "auto_update" => if let Some(v) = b { c.auto_update = v },
            "idle_webview" => if let Some(v) = b { c.idle_webview = v },
            "idle_hide_ui" => if let Some(v) = b { c.idle_hide_ui = v },
            "destroy_float" => if let Some(v) = b { c.destroy_float = v },
            "suspend_idle" => if let Some(v) = b { c.suspend_idle = v },
            "suspend_after_s" => if let Some(v) = n { c.suspend_after_s = (v as u64).clamp(30, 1800) },
            _ => {}
        }
    }
    match key.as_str() {
        "skin" => if let Some(v) = value.as_str() { set_skin(&app, v) },
        "scale" => if let Some(v) = n { set_float_scale(&app, v) },
        "source" => if let Some(v) = value.as_str() { set_source(&app, v) },
        "float_on_top" => if let (Some(w), Some(v)) = (app.get_webview_window("float"), b) { let _ = w.set_always_on_top(v); },
        "autostart" => if let Some(v) = b {
            let al = app.autolaunch();
            let _ = if v { al.enable() } else { al.disable() };
        },
        _ => {}
    }
    save_settings(&app);
}

#[tauri::command]
fn settings_action(app: AppHandle, action: String) {
    match action.as_str() {
        "pomo_start" => {
            let st = app.state::<AppState>();
            let (f, r) = { let c = st.settings.lock().unwrap(); (c.pomo_focus_min, c.pomo_rest_min) };
            start_pomo(&app, f, r);
        }
        "pomo_stop" => stop_pomo(&app),
        "check_update" => { let a = app.clone(); tauri::async_runtime::spawn(check_update(a, true)); }
        "install_update" => { tauri::async_runtime::spawn(install_update(app.clone())); }
        "reset_pos" => {
            if let Some(w) = app.get_webview_window("float") { let _ = w.center(); }
        }
        "reset_settings" => {
            {
                let st = app.state::<AppState>();
                let mut c = st.settings.lock().unwrap();
                let (source, welcomed) = (c.source.clone(), c.welcomed);
                *c = Settings { source, welcomed, ..Settings::default() };
            }
            save_settings(&app);
            set_float_scale(&app, 1.0);
            set_skin(&app, "dog");
            if let Some(w) = app.get_webview_window("float") { let _ = w.set_always_on_top(true); }
        }
        _ => {}
    }
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    #[allow(unused_mut)]
    let mut b = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Lite Music — Configuración")
        .inner_size(800.0, 600.0)
        .min_inner_size(660.0, 480.0)
        .center();
    #[cfg(windows)]
    {
        b = b.additional_browser_args(BROWSER_ARGS);
    }
    let _ = b.build();
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

// WebView2: con la ventana oculta, la marca como no visible y pide un uso de memoria bajo (como una pestaña en segundo plano).
#[cfg(windows)]
fn set_webview_idle(w: &WebviewWindow, idle: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL};
    use windows_core::Interface;
    let _ = w.with_webview(move |pw| unsafe {
        let controller = pw.controller();
        let _ = controller.SetIsVisible(!idle);
        if let Ok(core) = controller.CoreWebView2() {
            if let Ok(c19) = core.cast::<ICoreWebView2_19>() {
                let _ = c19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL(if idle { 1 } else { 0 }));
            }
        }
    });
}
#[cfg(not(windows))]
fn set_webview_idle(_w: &WebviewWindow, _idle: bool) {}

// Solo el nivel de memoria (sin tocar la visibilidad): sirve para "re-apretar" la memoria tras actividad.
#[cfg(windows)]
fn set_webview_memory(w: &WebviewWindow, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL};
    use windows_core::Interface;
    let _ = w.with_webview(move |pw| unsafe {
        if let Ok(core) = pw.controller().CoreWebView2() {
            if let Ok(c19) = core.cast::<ICoreWebView2_19>() {
                let _ = c19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL(if low { 1 } else { 0 }));
            }
        }
    });
}
#[cfg(not(windows))]
fn set_webview_memory(_w: &WebviewWindow, _low: bool) {}

// Suspende la página por completo (no ejecuta nada, como una pestaña congelada). Solo con la ventana oculta.
static SUSPENDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(windows)]
fn set_webview_suspended(w: &WebviewWindow, suspend: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
    use windows_core::Interface;
    let _ = w.with_webview(move |pw| unsafe {
        if let Ok(core) = pw.controller().CoreWebView2() {
            if let Ok(c3) = core.cast::<ICoreWebView2_3>() {
                if suspend {
                    let h = webview2_com::TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())));
                    let _ = c3.TrySuspend(&h);
                } else {
                    let _ = c3.Resume();
                }
            }
        }
    });
}
#[cfg(not(windows))]
fn set_webview_suspended(_w: &WebviewWindow, _suspend: bool) {}

// Despierta la página si estaba suspendida (antes de mostrarla o de enviarle una orden).
fn wake(app: &AppHandle) {
    if SUSPENDED.swap(false, std::sync::atomic::Ordering::SeqCst) {
        *app.state::<AppState>().last_report.lock().unwrap() = Instant::now();
        if let Some(w) = main_window(app) {
            set_webview_suspended(&w, false);
            set_webview_idle(&w, true);
        }
    }
}

// Entra o sale del modo ahorro de la ventana principal según los ajustes.
fn set_main_idle(app: &AppHandle, idle: bool) {
    let Some(w) = main_window(app) else { return };
    if !idle {
        wake(app);
    }
    let st = app.state::<AppState>();
    let (webview, ui) = {
        let c = st.settings.lock().unwrap();
        (c.idle_webview, c.idle_hide_ui)
    };
    let _ = w.eval(&format!("window.__lm&&window.__lm.setHidden({}, {})", idle, idle && ui));
    if webview || !idle {
        set_webview_idle(&w, idle);
    }
}

fn show(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        set_main_idle(app, false);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn hide(app: &AppHandle, w: &WebviewWindow) {
    let _ = w.hide();
    set_main_idle(app, true);
}

fn js(app: &AppHandle, code: &str) {
    if let Some(w) = main_window(app) {
        wake(app);
        let _ = w.eval(code);
    }
}

async fn tokio_sleep(secs: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_secs(secs))).await;
}

// Acciones de los menús (bandeja y barra superior de la ventana).
fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => show(app),
        "pp" => js(app, "window.__lm&&window.__lm.playPause()"),
        "next" => js(app, "window.__lm&&window.__lm.next()"),
        "prev" => js(app, "window.__lm&&window.__lm.prev()"),
        "src_ytm" => set_source(app, "ytm"),
        "src_spotify" => set_source(app, "spotify"),
        "float" => toggle_float(app),
        "sz50" => set_float_scale(app, 0.5),
        "sz70" => set_float_scale(app, 0.7),
        "sz100" => set_float_scale(app, 1.0),
        "dog" => set_skin(app, "dog"),
        "cat" => set_skin(app, "cat"),
        "cover" => set_skin(app, "cover"),
        "none" => set_skin(app, "none"),
        id @ ("trabajando" | "programando" | "tradeando" | "descansando" | "estudiando" | "festejando") => set_skin(app, id),
        "pomo25" => start_pomo(app, 25, 5),
        "pomo50" => start_pomo(app, 50, 10),
        "pomo0" => stop_pomo(app),
        "cfg" => open_settings(app),
        "upd" => {
            let a = app.clone();
            if update_pending() {
                tauri::async_runtime::spawn(install_update(a));
            } else {
                tauri::async_runtime::spawn(check_update(a, true));
            }
        }
        "update" => {
            tauri::async_runtime::spawn(install_update(app.clone()));
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![report_state, get_state, control, scale_float, get_settings, set_setting, settings_action, choose_source, open_link])
        .manage(AppState { pomo: Mutex::new(Pomo { phase: Phase::Off, end: Instant::now(), focus_s: 1500, rest_s: 300, prev_skin: String::new() }), user_float: Mutex::new(true), auto_hidden: Mutex::new(false), settings: Mutex::new(Settings::default()), pending_pos: Mutex::new(None), player: Mutex::new(PlayerState::default()), float_visible: Mutex::new(true), last_report: Mutex::new(Instant::now()) })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .on_menu_event(|app, ev| {
            if let Some(id) = ev.id.as_ref().strip_prefix("w_") {
                on_menu(app, id);
            }
        })
        .setup(|app| {
            *app.state::<AppState>().settings.lock().unwrap() = load_settings(app.handle());
            let start_url = source_url(&app.state::<AppState>().settings.lock().unwrap().source);
            #[allow(unused_mut)]
            let mut builder = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(start_url.parse().unwrap()),
            )
            .title("Lite Music")
            .visible(app.state::<AppState>().settings.lock().unwrap().welcomed)
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
            if !app.state::<AppState>().settings.lock().unwrap().welcomed {
                open_welcome(app.handle());
            }
            // Arranque con Windows: empieza oculta en la bandeja.
            if std::env::args().any(|a| a == "--minimized") {
                hide(app.handle(), &window);
            }

            // Reproductor flotante
            let cfg0 = app.state::<AppState>().settings.lock().unwrap().clone();
            set_skin(app.handle(), &cfg0.skin);
            if cfg0.float_at_start {
                create_float(app.handle())?;
            } else {
                *app.state::<AppState>().user_float.lock().unwrap() = false;
                *app.state::<AppState>().float_visible.lock().unwrap() = false;
            }

            // Cerrar = ocultar a la bandeja; la música sigue sonando.
            let w = window.clone();
            let wapp = app.handle().clone();
            window.on_window_event(move |e| {
                if let WindowEvent::CloseRequested { api, .. } = e {
                    if wapp.state::<AppState>().settings.lock().unwrap().close_to_tray {
                        api.prevent_close();
                        hide(&wapp, &w);
                    } else {
                        wapp.exit(0);
                    }
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
            let s50 = MenuItem::with_id(app, "sz50", "Muy pequeño (50%)", true, None::<&str>)?;
            let s70 = MenuItem::with_id(app, "sz70", "Pequeño (70%)", true, None::<&str>)?;
            let s100 = MenuItem::with_id(app, "sz100", "Normal (100%)", true, None::<&str>)?;
            let m_size = Submenu::with_items(app, "Tamaño del flotante", true, &[&s50, &s70, &s100])?;
            let src_y = MenuItem::with_id(app, "src_ytm", "YouTube Music", true, None::<&str>)?;
            let src_s = MenuItem::with_id(app, "src_spotify", "Spotify", true, None::<&str>)?;
            let m_src = Submenu::with_items(app, "Servicio de música", true, &[&src_y, &src_s])?;
            let m_cfg = MenuItem::with_id(app, "cfg", "Configuración…", true, None::<&str>)?;
            let m_upd = MenuItem::with_id(app, "upd", "Buscar actualizaciones", true, None::<&str>)?;
            let m_upd_tray = m_upd.clone();
            let p1 = MenuItem::with_id(app, "pomo25", "Iniciar 25 min estudio / 5 descanso", true, None::<&str>)?;
            let p2 = MenuItem::with_id(app, "pomo50", "Iniciar 50 min estudio / 10 descanso", true, None::<&str>)?;
            let p3 = MenuItem::with_id(app, "pomo0", "Detener", true, None::<&str>)?;
            let m_pomo = Submenu::with_items(app, "Pomodoro", true, &[&p1, &p2, &p3])?;
            let m_quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&m_show, &sep, &m_pp, &m_prev, &m_next, &sep, &m_src, &m_float, &m_skin, &m_size, &m_pomo, &sep, &m_cfg, &m_upd, &sep, &m_quit])?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Lite Music")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| on_menu(app, ev.id.as_ref()))
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

            // Barra superior de la ventana: Configuración, Aspecto, Servicio y Pomodoro (nativa: no tapa nada de la página).
            let mi = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
            let bar = Menu::new(app)?;
            bar.append(&mi("w_cfg", "⚙  Configuración")?)?;
            let sm_skin = Submenu::new(app, "🎨  Aspecto", true)?;
            for (id, t) in [("dog", "🐶 Perro"), ("cat", "🐱 Gato"), ("cover", "💿 Carátula"), ("none", "▭ Solo controles")] {
                sm_skin.append(&mi(&format!("w_{id}"), t)?)?;
            }
            sm_skin.append(&PredefinedMenuItem::separator(app)?)?;
            for (id, t) in [("trabajando", "💼 Trabajando"), ("programando", "💻 Programando"), ("tradeando", "📈 Tradeando"),
                            ("descansando", "🛏 Descansando"), ("estudiando", "📚 Estudiando"), ("festejando", "🎉 Festejando")] {
                sm_skin.append(&mi(&format!("w_{id}"), t)?)?;
            }
            bar.append(&sm_skin)?;
            let sm_src = Submenu::new(app, "🎵  Servicio", true)?;
            sm_src.append(&mi("w_src_ytm", "YouTube Music")?)?;
            sm_src.append(&mi("w_src_spotify", "Spotify")?)?;
            bar.append(&sm_src)?;
            let sm_pomo = Submenu::new(app, "🍅  Pomodoro", true)?;
            sm_pomo.append(&mi("w_pomo25", "Iniciar 25 min estudio / 5 descanso")?)?;
            sm_pomo.append(&mi("w_pomo50", "Iniciar 50 min estudio / 10 descanso")?)?;
            sm_pomo.append(&mi("w_pomo0", "Detener")?)?;
            bar.append(&sm_pomo)?;
            let _ = window.set_menu(bar.clone());
            let _ = UPD.set(Mutex::new(UpdUi { menu: bar, bar_item: mi("w_update", "⬆  Nueva versión disponible")?, tray_item: m_upd_tray, shown: false, version: None }));

            start_ticker(app.handle().clone());

            // Búsqueda de versiones nuevas (solo avisa): al abrir, tras unos segundos, y luego cada 6 horas.
            let a = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio_sleep(8).await;
                loop {
                    if a.state::<AppState>().settings.lock().unwrap().auto_update {
                        check_update(a.clone(), false).await;
                    }
                    tokio_sleep(6 * 3600).await;
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al iniciar Lite Music");
}
