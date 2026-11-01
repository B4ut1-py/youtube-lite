#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, sync::{atomic::{AtomicBool, Ordering::SeqCst}, Mutex}, time::{Duration, Instant}};

use serde::{Deserialize, Serialize};
use tauri_plugin_updater::UpdaterExt;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    LogicalPosition, LogicalSize, PhysicalPosition,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::WebviewBuilder,
    window::WindowBuilder,
    AppHandle, Manager, Webview, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent,
};

const YTM_URL: &str = "https://music.youtube.com";
const SPOTIFY_URL: &str = "https://open.spotify.com";

fn source_url(source: &str) -> &'static str {
    if source == "spotify" { SPOTIFY_URL } else { YTM_URL }
}
// User-Agent de Chrome: evita que Google bloquee el inicio de sesión por "navegador no seguro".
// Lleva la versión real del motor: una fija queda vieja y no coincide con la que WebView2 informa por otras vías.
fn user_agent() -> String {
    let major = tauri::webview_version().ok().and_then(|v| v.split('.').next().map(str::to_owned)).unwrap_or_else(|| "130".into());
    format!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36")
}

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
    source: String,
    welcomed: bool,
    notify_song: bool,
    theme: String,  // "auto" (sigue a Windows), "dark" o "light"
    glass: String,  // estilo del flotante: "off", "art" (cristal sobre la carátula) o "screen" (transparente sobre la pantalla)
    format: String, // formato del flotante: "normal", "compact" (una fila) o "mini" (cápsula)
    float_app_bg: bool,  // el flotante con Cristal usa el fondo de la app en lugar de la carátula
    audio_only: bool,    // sin videoclips (YouTube Music) ni videos de fondo (Canvas de Spotify): solo la música
    glass_bg: String,    // fondo fijo del Cristal en la app: "dark", "light" o "custom"
    glass_color: String, // color del fondo cuando glass_bg es "custom" (#rrggbb)
    #[serde(default)] // ausente en archivos viejos = false (el default de la estructura es true para instalaciones nuevas)
    float_v2: bool, // ya se adaptó la escala al flotante nuevo (tarjeta)
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            skin: "dog".into(), scale: 1.0, autohide: true, float_opacity: 1.0, float_on_top: true, float_at_start: true,
            blink: true, reactions: true, wave: true, wave_min_s: 25, wave_max_s: 55, yawn: true, yawn_idle_s: 90,
            mood_min_s: 15, mood_max_s: 30, pomo_focus_min: 25, pomo_rest_min: 5, close_to_tray: true, auto_update: true,
            idle_webview: true, idle_hide_ui: true, destroy_float: true, source: "ytm".into(), welcomed: false, notify_song: false, theme: "auto".into(), glass: "off".into(), format: "normal".into(), float_app_bg: false, audio_only: true, glass_bg: "dark".into(), glass_color: "#5b4bd6".into(), float_v2: true,
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
    #[serde(default)]
    liked: bool,
    sleep_left: u64,
    cfg: Settings,
    #[serde(default)]
    glass: serde_json::Value, // ver glass_spec: el flotante lo usa si toma el fondo de la app
    #[serde(default)]
    thin: bool, // isla finita (ver ISL_THIN)
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
    pos_at: Mutex<Instant>,        // cuándo se informó por última vez el tiempo de la canción
    last_report: Mutex<Instant>,   // última vez que la página de música dio señales de vida
    sleep_at: Mutex<Option<Instant>>, // temporizador para dormir: cuándo pausar
    search: Mutex<(u64, Option<serde_json::Value>)>, // búsqueda rápida: número de la última y su respuesta
}

// Isla desplegada (al pasar el mouse): 380x136 más las aletas y la sombra.
const ISLAND_OPEN: (f64, f64) = (424.0, 150.0);

// Isla finita: con una ventana maximizada en la pantalla principal la isla cerrada se reduce a una muesca con las barritas
// y deja pasar los clics (no tapa las pestañas del navegador). Se despliega con el mouse quieto arriba (island_hover).
static ISL_THIN: AtomicBool = AtomicBool::new(false);
static ISL_OPEN: AtomicBool = AtomicBool::new(false);

// La isla va pegada al borde de arriba, centrada en la pantalla principal (no se arrastra).
fn place_island(w: &WebviewWindow, size: LogicalSize<f64>) {
    let Some(m) = w.primary_monitor().ok().flatten() else { return };
    let phys = size.to_physical::<i32>(m.scale_factor());
    let _ = w.set_position(PhysicalPosition::new(m.position().x + (m.size().width as i32 - phys.width) / 2, m.position().y));
}

// Tamaño de la ventana del flotante según su formato: la tarjeta más margen para la sombra y la etiqueta de arriba.
fn float_size(format: &str, scale: f64) -> LogicalSize<f64> {
    let (w, h) = match format {
        "compact" => (312.0, 92.0), // tarjeta de 288x64
        "mini" => (200.0, 68.0),    // cápsula de 176x40
        "island" if ISL_THIN.load(SeqCst) => (84.0, 22.0), // isla finita: 64x16 más las aletas
        "island" => (258.0, 44.0),  // isla cerrada: 230x34 más las aletas que la unen al borde (la abierta, ISLAND_OPEN)
        _ => (312.0, 144.0),        // tarjeta de 288x116
    };
    LogicalSize::new(w * scale, h * scale)
}

fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn save_settings(app: &AppHandle) {
    let st = app.state::<AppState>();
    let c = st.settings.lock().unwrap(); // tomado hasta terminar: dos guardados a la vez no se mezclan
    let json = serde_json::to_string_pretty(&*c).unwrap_or_default();
    if let Some(p) = settings_path(app) {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        // Se escribe aparte y se renombra: un cierre a mitad de escritura no deja settings.json roto
        // (se perderían todos los ajustes y volvería la pantalla de bienvenida).
        let tmp = p.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &p);
        }
    }
}

// Sin la marca BOM que agregan algunos editores: con ella el archivo no se lee y se pierden todos los ajustes.
fn parse_settings(text: &str) -> Option<Settings> {
    let mut s = serde_json::from_str::<Settings>(text.trim_start_matches('\u{feff}')).ok()?;
    if !s.float_v2 {
        // La escala guardada era para el flotante anterior, mucho más grande: con la tarjeta nueva se empieza en 100 %.
        s.scale = 1.0;
        s.float_v2 = true;
    }
    Some(s)
}

// Lee settings.json; si no existe (versiones anteriores) toma lo guardado en los archivos sueltos.
fn load_settings(app: &AppHandle) -> Settings {
    if let Some(s) = settings_path(app).and_then(|p| fs::read_to_string(p).ok()).and_then(|t| parse_settings(&t)) {
        return s;
    }
    let mut s = Settings::default();
    if let Some(v) = cfg_read(app, "skin.txt").filter(|v| SKINS.contains(&v.as_str())) { s.skin = v; }
    
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
    if let Some(w) = main_window(app) {
        let _ = w.navigate(source_url(source).parse().unwrap());
        show(app);
        // Cuando la página nueva ya cargó, se apaga lo que quedó vivo del servicio anterior.
        let a = app.clone();
        std::thread::spawn(move || {
            for secs in [6, 25] {
                std::thread::sleep(Duration::from_secs(secs));
                if let Some(w) = main_window(&a) {
                    stop_service_workers(&w);
                }
            }
        });
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
        "github" => "https://github.com/bautista-rubio",
        "repo" => "https://github.com/bautista-rubio/lite-music",
        "linkedin" => "https://www.linkedin.com/in/bautista-rubio/",
        _ => return,
    };
    open_in_browser(url);
}

// Sitios que pueden abrirse dentro de la ventana: los servicios y sus inicios de sesión. El resto va al navegador:
// sin barra de direcciones no se sabría en qué página se está (p. ej. un inicio de sesión falso).
fn allowed_in_app(u: &tauri::Url) -> bool {
    if u.scheme() == "about" {
        return true;
    }
    let (true, Some(host)) = (u.scheme() == "https", u.host_str()) else { return false };
    let labels: Vec<&str> = host.split('.').collect();
    // google.com, google.com.ar, google.co.uk, google.de...: Google inicia sesión pasando por el dominio de cada país.
    let google = labels.iter().position(|l| *l == "google").map_or(false, |i| match &labels[i + 1..] {
        ["com"] => true,
        [cc] | ["com" | "co", cc] => cc.len() == 2,
        _ => false,
    });
    google || ["youtube.com", "spotify.com", "facebook.com", "apple.com"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

// Abre una dirección web en el navegador predeterminado (solo http/https: nunca un archivo o programa).
fn open_in_browser(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    // ShellExecute y no explorer.exe: explorer lee las comas de la dirección como opciones propias
    // (",/root,shell:Downloads" abría una carpeta de la PC en vez del navegador).
    #[cfg(windows)]
    unsafe {
        use windows::{core::{w, HSTRING}, Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL}};
        ShellExecuteW(None, w!("open"), &HSTRING::from(url), None, None, SW_SHOWNORMAL);
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
    let scale = scale.clamp(0.7, 1.6);
    let st = app.state::<AppState>();
    let format = { let mut c = st.settings.lock().unwrap(); c.scale = scale; c.format.clone() };
    if let Some(w) = app.get_webview_window("float") {
        let size = float_size(&format, scale);
        let _ = w.set_size(size);
        let _ = w.set_zoom(scale);
        if format == "island" { place_island(&w, size); }
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
    let size = float_size(&cfg.format, cfg.scale);
    #[allow(unused_mut)]
    let mut fb = WebviewWindowBuilder::new(app, "float", WebviewUrl::App("float.html".into()))
        .title("Lite Music Float")
        .inner_size(size.width, size.height)
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
    reload_on_crash(app, w.as_ref());
    set_webview_memory(w.as_ref(), true); // página chiquita siempre a la vista: no necesita las cachés de una pestaña normal
    let _ = w.set_zoom(cfg.scale);
    if cfg.format == "island" { place_island(&w, size); } else { restore_float_pos(app, &w); }
    let ah = app.clone();
    w.on_window_event(move |e| {
        // la isla tiene lugar fijo: no pisa la posición guardada del flotante
        if let WindowEvent::Moved(p) = e {
            if ah.state::<AppState>().settings.lock().unwrap().format != "island" {
                *ah.state::<AppState>().pending_pos.lock().unwrap() = Some((p.x, p.y));
            }
        }
    });
    Ok(w)
}

static FLOAT_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn float_wanted(app: &AppHandle) -> bool {
    let st = app.state::<AppState>();
    let want = *st.user_float.lock().unwrap() && !*st.auto_hidden.lock().unwrap();
    want // en una variable: los bloqueos se sueltan antes de devolver
}

// ¿Coincide lo que se ve con lo que se quiere ver? (el latido de cada segundo lo corrige si no)
fn float_in_sync(app: &AppHandle) -> bool {
    let want = float_wanted(app);
    let destroy = app.state::<AppState>().settings.lock().unwrap().destroy_float;
    match app.get_webview_window("float") {
        Some(w) => want == w.is_visible().unwrap_or(true),
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
                    // la isla tiene lugar fijo: no pisa la posición guardada del flotante
                    let island = a.state::<AppState>().settings.lock().unwrap().format == "island";
                    if let (false, Ok(p)) = (island, w.outer_position()) {
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
    let monitors = app.available_monitors().unwrap_or_default();
    let Some(m) = monitors.iter().find(|m| {
        let (p, z) = (m.position(), m.size());
        x + 40 >= p.x && x + 40 < p.x + z.width as i32 && y + 40 >= p.y && y + 40 < p.y + z.height as i32
    }) else { return };
    // Entero dentro del monitor: la posición guardada puede venir de un flotante de otro tamaño.
    let (p, z) = (m.position(), m.size());
    let ws = w.outer_size().unwrap_or_default();
    let x = x.clamp(p.x, (p.x + z.width as i32 - ws.width as i32).max(p.x));
    let y = y.clamp(p.y, (p.y + z.height as i32 - ws.height as i32).max(p.y));
    let _ = w.set_position(PhysicalPosition::new(x, y));
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

// ¿La ventana que está justo debajo de la isla (arriba al centro de la pantalla principal) está maximizada? Se recorren
// las ventanas de adelante hacia atrás sin depender de cuál está activa: un clic en la barra de tareas o en la isla no cambia
// lo que hay debajo. Se saltan el flotante y la búsqueda (la ventana de la app sí cuenta), las ocultas y las "fantasma".
#[cfg(windows)]
fn maximized_under_island(app: &AppHandle) -> bool {
    use windows::Win32::{Foundation::{HWND, RECT}, Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
        UI::WindowsAndMessaging::{GetTopWindow, GetWindow, GetWindowRect, IsWindowVisible, IsZoomed, GW_HWNDNEXT}};
    let Some(m) = app.primary_monitor().ok().flatten() else { return false };
    let (x, y) = (m.position().x + m.size().width as i32 / 2, m.position().y + 2);
    let skip: Vec<isize> = ["float", "search"].iter().filter_map(|l| app.get_webview_window(l)?.hwnd().ok()).map(|h| h.0 as isize).collect();
    unsafe {
        let mut h = GetTopWindow(None).unwrap_or(HWND::default());
        while !h.is_invalid() {
            let (mut r, mut cloaked) = (RECT::default(), 0u32);
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as _, 4);
            if !skip.contains(&(h.0 as isize)) && IsWindowVisible(h).as_bool() && cloaked == 0
                && GetWindowRect(h, &mut r).is_ok() && x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                return IsZoomed(h).as_bool();
            }
            h = GetWindow(h, GW_HWNDNEXT).unwrap_or(HWND::default());
        }
    }
    false
}
#[cfg(not(windows))]
fn maximized_under_island(_app: &AppHandle) -> bool {
    false
}

// Isla finita: con el mouse quieto medio segundo sobre ella se despliega; abierta, se repliega cuando el mouse sale
// (los clics la atraviesan, así que la página no ve entrar el mouse: se mira la posición desde aquí).
fn island_hover(app: AppHandle) {
    std::thread::spawn(move || {
        let mut since: Option<Instant> = None;
        loop {
            std::thread::sleep(Duration::from_millis(if ISL_THIN.load(SeqCst) { 100 } else { 1000 }));
            let (true, Some(w), Ok(c)) = (ISL_THIN.load(SeqCst), app.get_webview_window("float"), app.cursor_position()) else { since = None; continue };
            let (Ok(p), Ok(s)) = (w.outer_position(), w.outer_size()) else { continue };
            let inside = c.x >= p.x as f64 && c.x < p.x as f64 + s.width as f64 && c.y >= p.y as f64 && c.y < p.y as f64 + s.height as f64;
            let open = ISL_OPEN.load(SeqCst);
            if inside == open { since = None; continue; }   // cerrada cuenta el tiempo adentro; abierta, el tiempo afuera
            if since.get_or_insert_with(Instant::now).elapsed() < Duration::from_millis(if open { 400 } else { 500 }) { continue; }
            since = None;
            if open {
                let _ = w.eval("window.__islClose&&__islClose()");
                std::thread::sleep(Duration::from_secs(1)); // el repliegue tarda ~0,7 s: no se lo reinicia
            } else {
                ISL_OPEN.store(true, SeqCst);
                let _ = w.set_ignore_cursor_events(false);
                let _ = w.eval("window.__islOpen&&__islOpen()");
            }
        }
    });
}

// --- Pomodoro: estudio -> descanso -> estudio... hasta que lo detengas ---
fn set_skin_temp(app: &AppHandle, skin: &str) {
    app.state::<AppState>().player.lock().unwrap().skin = skin.into();
}

fn start_pomo(app: &AppHandle, focus_min: u64, rest_min: u64) {
    {
        let st = app.state::<AppState>();
        let mut p = st.pomo.lock().unwrap();
        if p.phase == Phase::Off {
            p.prev_skin = st.player.lock().unwrap().skin.clone();
        }
        p.phase = Phase::Focus;
        p.focus_s = focus_min * 60;
        p.rest_s = rest_min * 60;
        p.end = Instant::now() + Duration::from_secs(p.focus_s);
    }
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
    refresh_tooltip(app);
}

fn pomo_tick(app: &AppHandle) {
    let changed = {
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
        changed
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
    refresh_tooltip(app);
}

// Un solo hilo con un latido por segundo: pomodoro, ocultarse en pantalla completa y guardar la posición.
fn start_ticker(app: AppHandle) {
    std::thread::spawn(move || {
    let mut last_title = String::new();
    let mut trim_at: Option<Instant> = None;
    let mut last_trim = Instant::now();
    let mut last_reload = Instant::now();
    let mut was_hidden = false;
    let mut extra_trim: Vec<Instant> = Vec::new();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        // Ventana oculta: mientras suena la página vuelve a crecer (cambio de canción, buffers); se le pide bajar la memoria cada 30 s.
        if let Some(w) = main_window(&app) {
            let st = app.state::<AppState>();
            let enabled = st.settings.lock().unwrap().idle_webview;
            let hidden = !w.window().is_visible().unwrap_or(true) || MINIMIZED.load(std::sync::atomic::Ordering::SeqCst);
            // Vigilante: si la página lleva un rato sin dar señales (se cortó la conexión y quedó la pantalla de error,
            // o la página murió en segundo plano), se recarga sola. No toca la página de inicio de sesión.
            let (welcomed, source) = {
                let c = st.settings.lock().unwrap();
                (c.welcomed, c.source.clone())
            };
            let on_service = w.url().map_or(false, |u| {
                matches!(u.host_str(), Some("music.youtube.com") | Some("open.spotify.com")) || !matches!(u.scheme(), "http" | "https")
            });
            if welcomed && on_service
                && st.last_report.lock().unwrap().elapsed() > Duration::from_secs(150)
                && last_reload.elapsed() > Duration::from_secs(60)
            {
                let _ = w.navigate(source_url(&source).parse().unwrap());
                last_reload = Instant::now();
                *st.last_report.lock().unwrap() = Instant::now();
            }
            // Al ocultar la ventana se recorta enseguida (a los 5 y a los 30 s), sin esperar al ciclo periódico.
            if hidden && !was_hidden {
                let now = Instant::now();
                extra_trim = vec![now + Duration::from_secs(5), now + Duration::from_secs(30)];
            }
            was_hidden = hidden;
            if enabled && hidden {
                let now = Instant::now();
                if extra_trim.iter().any(|t| now >= *t) {
                    extra_trim.retain(|t| now < *t);
                    trim_at = Some(now);
                }
                let title = st.player.lock().unwrap().title.clone();
                if title != last_title {
                    last_title = title;
                    trim_at = Some(Instant::now() + Duration::from_secs(10));
                }
                let due = trim_at.map_or(false, |t| Instant::now() >= t) || last_trim.elapsed() > Duration::from_secs(30);
                if due {
                    // Oculta, la página no dibuja y YouTube Music deja en cada canción trabajo "para el próximo cuadro"
                    // que retiene ~10 MB y nunca se suelta: se la deja dibujar 1 s (la página está en blanco, no se ve nada).
                    if trim_at.is_some() {
                        set_webview_visible(&w, true);
                        std::thread::sleep(Duration::from_secs(1));
                        set_webview_visible(&w, false);
                    }
                    stop_service_workers(&w);
                    let float = app.get_webview_window("float");
                    set_webview_memory(&w, false);
                    if let Some(f) = &float { set_webview_memory(f.as_ref(), false); }
                    std::thread::sleep(Duration::from_millis(300));
                    set_webview_memory(&w, true);
                    if let Some(f) = &float { set_webview_memory(f.as_ref(), true); }
                    trim_at = None;
                    last_trim = Instant::now();
                }
            } else {
                last_trim = Instant::now();
            }
        }
        pomo_tick(&app);
        let st = app.state::<AppState>();
        let sleep_due = {
            let mut s = st.sleep_at.lock().unwrap();
            let due = s.map_or(false, |t| Instant::now() >= t);
            if due {
                *s = None;
            }
            due
        };
        if sleep_due {
            js(&app, "window.__lm&&window.__lm.pause()");
        }
        if sleep_due || st.sleep_at.lock().unwrap().is_some() {
            refresh_tooltip(&app);
        }
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
        let thin = st.settings.lock().unwrap().format == "island" && maximized_under_island(&app);
        ISL_THIN.store(thin, SeqCst);
        if let Some(f) = app.get_webview_window("float") {
            let _ = f.set_ignore_cursor_events(thin && !ISL_OPEN.load(SeqCst));
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
fn report_state(app: AppHandle, state: tauri::State<AppState>, title: String, artist: String, cover: String, vol: f64, playing: bool, pos: f64, dur: f64, liked: bool) -> bool {
    *state.last_report.lock().unwrap() = Instant::now();
    *state.pos_at.lock().unwrap() = Instant::now();
    let (new_song, play_changed) = {
        let mut p = state.player.lock().unwrap();
        let new_song = !title.is_empty() && (title != p.title || artist != p.artist);
        let play_changed = playing != p.playing;
        p.title = title;
        p.artist = artist;
        p.cover = cover;
        p.vol = vol;
        p.playing = playing;
        p.pos = pos;
        p.dur = dur;
        p.liked = liked;
        (new_song, play_changed)
    };
    if new_song {
        refresh_tooltip(&app);
        notify_song(&app);
    }
    if play_changed {
        thumbbar::update(&app, playing);
    }
    *state.float_visible.lock().unwrap()
}

// Aviso de Windows con la canción nueva (opcional), solo si no estás mirando la ventana de la app.
fn notify_song(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    if !app.state::<AppState>().settings.lock().unwrap().notify_song || main_win(app).map_or(false, |w| w.is_focused().unwrap_or(false)) {
        return;
    }
    let (title, artist) = {
        let st = app.state::<AppState>();
        let p = st.player.lock().unwrap();
        (p.title.clone(), p.artist.clone())
    };
    let _ = app.notification().builder().title(title).body(artist).show();
}

// Texto al pasar el mouse por el ícono de la bandeja: canción actual, Pomodoro y temporizador para dormir.
static LAST_TIP: Mutex<String> = Mutex::new(String::new());
fn refresh_tooltip(app: &AppHandle) {
    let st = app.state::<AppState>();
    let mut t = {
        let p = st.player.lock().unwrap();
        match (p.title.is_empty(), p.artist.is_empty()) {
            (true, _) => "Lite Music".to_string(),
            (false, true) => p.title.clone(),
            _ => format!("{} — {}", p.title, p.artist),
        }
    };
    {
        let pm = st.pomo.lock().unwrap();
        if pm.phase != Phase::Off {
            let left = pm.end.saturating_duration_since(Instant::now()).as_secs();
            t += &format!("\n{} {}:{:02}", if pm.phase == Phase::Focus { "Estudio" } else { "Descanso" }, left / 60, left % 60);
        }
    }
    if let Some(at) = *st.sleep_at.lock().unwrap() {
        t += &format!("\nSe pausa en {} min", at.saturating_duration_since(Instant::now()).as_secs().div_ceil(60));
    }
    if let Some(v) = UPD.get().and_then(|u| u.lock().unwrap().version.clone()) {
        t += &format!("\nVersión {v} disponible");
    }
    let t: String = t.chars().take(120).collect(); // Windows corta el texto en 127 caracteres
    let mut last = LAST_TIP.lock().unwrap();
    if *last != t {
        if let Some(tr) = app.tray_by_id("main") {
            let _ = tr.set_tooltip(Some(&t));
        }
        *last = t;
    }
}

// Temporizador para dormir: pausa la música dentro de `min` minutos (0 = cancelar).
fn set_sleep(app: &AppHandle, min: u64) {
    let min = min.min(24 * 60); // tope: un número enorme desbordaría la hora y cerraría la app
    *app.state::<AppState>().sleep_at.lock().unwrap() = (min > 0).then(|| Instant::now() + Duration::from_secs(min * 60));
    refresh_tooltip(app);
}

#[tauri::command]
fn get_state(state: tauri::State<AppState>) -> PlayerState {
    let mut p = state.player.lock().unwrap().clone();
    // La página ya no informa cada 0,5 s: el tiempo se calcula aquí a partir del último informe.
    if p.playing {
        let el = state.pos_at.lock().unwrap().elapsed().as_secs_f64().min(120.0);
        p.pos = if p.dur > 0.0 { (p.pos + el).min(p.dur) } else { p.pos + el };
    }
    let pm = state.pomo.lock().unwrap();
    p.pomo_phase = match pm.phase { Phase::Off => "off", Phase::Focus => "focus", Phase::Rest => "rest" }.into();
    p.cfg = state.settings.lock().unwrap().clone();
    p.glass = glass_spec(&p.cfg);
    p.thin = ISL_THIN.load(SeqCst);
    p.pomo_left = if pm.phase == Phase::Off { 0 } else { pm.end.saturating_duration_since(Instant::now()).as_secs() };
    p.sleep_left = state.sleep_at.lock().unwrap().map_or(0, |t| t.saturating_duration_since(Instant::now()).as_secs());
    p
}

#[tauri::command]
fn control(app: AppHandle, action: String, value: Option<f64>) {
    if action == "search" { return open_search(&app); }
    if action == "island" {   // la isla se despliega (1) o se repliega (0): la ventana crece o se achica desde el centro de arriba
        let st = app.state::<AppState>();
        let (format, scale) = { let c = st.settings.lock().unwrap(); (c.format.clone(), c.scale) };
        if let (true, Some(w)) = (format == "island", app.get_webview_window("float")) {
            let open = value.unwrap_or(0.0) > 0.5;
            ISL_OPEN.store(open, SeqCst);
            let size = if open { LogicalSize::new(ISLAND_OPEN.0 * scale, ISLAND_OPEN.1 * scale) } else { float_size("island", scale) };
            let _ = w.set_size(size);
            place_island(&w, size);
            let _ = w.set_ignore_cursor_events(!open && ISL_THIN.load(SeqCst));
        }
        return;
    }
    let code = match action.as_str() {
        "playpause" => "window.__lm&&window.__lm.playPause()".to_string(),
        "next" => "window.__lm&&window.__lm.next()".to_string(),
        "prev" => "window.__lm&&window.__lm.prev()".to_string(),
        "volume" => format!("window.__lm&&window.__lm.setVolume({})", value.unwrap_or(1.0).clamp(0.0, 1.0)),
        "seek" => format!("window.__lm&&window.__lm.seek({})", value.unwrap_or(0.0).clamp(0.0, 1.0)),
        "like" => "window.__lm&&window.__lm.like&&window.__lm.like()".to_string(),
        "volume_step" => {
            let v = app.state::<AppState>().player.lock().unwrap().vol + value.unwrap_or(0.0);
            format!("window.__lm&&window.__lm.setVolume({})", v.clamp(0.0, 1.0))
        }
        _ => return,
    };
    js(&app, &code);
    js(&app, "window.__lm&&window.__lm.kick&&window.__lm.kick(400)");
}

// Versión nueva disponible: se muestra en la barra de la ventana (bar_state) y en el menú de la bandeja.
struct UpdUi {
    tray_item: MenuItem<tauri::Wry>,
    version: Option<String>,
}
static UPD: std::sync::OnceLock<Mutex<UpdUi>> = std::sync::OnceLock::new();

fn set_update_available(app: &AppHandle, version: Option<String>) {
    let Some(u) = UPD.get() else { return };
    let mut u = u.lock().unwrap();
    let _ = match &version {
        Some(v) => u.tray_item.set_text(format!("⬆ Instalar versión {v}")),
        None => u.tray_item.set_text("Buscar actualizaciones"),
    };
    u.version = version;
    drop(u);
    refresh_tooltip(app);
}

fn update_pending() -> bool {
    UPD.get().map_or(false, |u| u.lock().unwrap().version.is_some())
}

// Mensaje pasajero en el ícono de la bandeja; la próxima actualización del texto vuelve a mostrar la canción.
fn say(app: &AppHandle, msg: &str) {
    if let Some(t) = app.tray_by_id("main") {
        let _ = t.set_tooltip(Some(msg));
    }
    LAST_TIP.lock().unwrap().clear();
}

// Busca una versión nueva en GitHub y AVISA en la ventana y en la bandeja. Nunca instala por su cuenta.
// Devuelve si se pudo consultar.
async fn check_update(app: AppHandle, manual: bool) -> bool {
    let Ok(updater) = app.updater() else { return false };
    match updater.check().await {
        Ok(Some(update)) => {
            set_update_available(&app, Some(update.version));
            true
        }
        Ok(None) => {
            set_update_available(&app, None);
            if manual {
                say(&app, "Lite Music: ya tienes la última versión");
            }
            true
        }
        Err(_) => {
            if manual {
                say(&app, "Lite Music: no se pudo buscar actualizaciones");
            }
            false
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

// Lo que muestra la barra de la ventana.
#[derive(Serialize)]
struct BarState {
    source: String,
    float: bool,
    sleep_left: u64,
    pomo_phase: &'static str,
    pomo_left: u64,
    update: Option<String>,
    theme: String,
    glass: serde_json::Value, // ver glass_spec
    maximized: bool,
}

#[tauri::command]
fn bar_state(app: AppHandle) -> BarState {
    let st = app.state::<AppState>();
    let (source, theme, glass) = { let c = st.settings.lock().unwrap(); (c.source.clone(), c.theme.clone(), glass_spec(&c)) };
    let (pomo_phase, pomo_left) = {
        let pm = st.pomo.lock().unwrap();
        match pm.phase {
            Phase::Off => ("off", 0),
            ph => (if ph == Phase::Focus { "focus" } else { "rest" }, pm.end.saturating_duration_since(Instant::now()).as_secs()),
        }
    };
    let sleep_left = st.sleep_at.lock().unwrap().map_or(0, |t| t.saturating_duration_since(Instant::now()).as_secs());
    let float = *st.user_float.lock().unwrap();
    BarState {
        source,
        float,
        sleep_left,
        pomo_phase,
        pomo_left,
        update: UPD.get().and_then(|u| u.lock().unwrap().version.clone()),
        theme,
        glass,
        maximized: main_win(&app).and_then(|w| w.is_maximized().ok()).unwrap_or(false),
    }
}

// Botones de la barra: solo estas acciones (nada de "salir" ni otras del menú).
// Asíncrono: un comando síncrono corre en el hilo principal y abrir Configuración desde ahí lo deja bloqueado.
#[tauri::command]
async fn bar_action(app: AppHandle, action: String) {
    const OK: [&str; 14] = ["src_ytm", "src_spotify", "float", "cfg", "upd", "pomo25", "pomo50", "pomo0", "sleep15", "sleep30", "sleep60", "sleep90", "sleep0", "show"];
    if OK.contains(&action.as_str()) {
        on_menu(&app, &action);
    }
}

// Cristal de la app (YouTube Music, Spotify y ventanas propias): null si está apagado, o { tone, tint }.
// El tono (texto claro u oscuro) sale del brillo del color de fondo.
fn glass_spec(c: &Settings) -> serde_json::Value {
    if c.glass == "off" { return serde_json::Value::Null; }
    let tint = match c.glass_bg.as_str() { "light" => "#dfe3ee", "custom" => c.glass_color.as_str(), _ => "#262a3d" };
    let rgb = u32::from_str_radix(tint.trim_start_matches('#'), 16).unwrap_or(0);
    let lum = 0.2126 * ((rgb >> 16) & 255) as f64 + 0.7152 * ((rgb >> 8) & 255) as f64 + 0.0722 * (rgb & 255) as f64;
    serde_json::json!({ "tone": if lum > 150.0 { "light" } else { "dark" }, "tint": tint })
}

#[derive(Serialize)]
struct SettingsView {
    settings: Settings,
    glass_spec: serde_json::Value,
    autostart: bool,
    version: String,
}

#[tauri::command]
fn get_settings(app: AppHandle) -> SettingsView {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();   // un solo bloqueo: dos en la misma expresión se traban
    SettingsView {
        glass_spec: glass_spec(&settings),
        settings,
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
            "notify_song" => if let Some(v) = b { c.notify_song = v },
            "theme" => if let Some(v) = value.as_str().filter(|v| ["auto", "dark", "light"].contains(v)) { c.theme = v.into() },
            "glass" => if let Some(v) = value.as_str().filter(|v| ["off", "art", "screen"].contains(v)) { c.glass = v.into() },
            "format" => if let Some(v) = value.as_str().filter(|v| ["normal", "compact", "mini", "island"].contains(v)) { c.format = v.into() },
            "float_app_bg" => if let Some(v) = b { c.float_app_bg = v },
            "audio_only" => if let Some(v) = b { c.audio_only = v },
            "glass_bg" => if let Some(v) = value.as_str().filter(|v| ["dark", "light", "custom"].contains(v)) { c.glass_bg = v.into() },
            "glass_color" => if let Some(v) = value.as_str().filter(|v| v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|h| h.is_ascii_hexdigit())) { c.glass_color = v.into() },
            "auto_update" => if let Some(v) = b { c.auto_update = v },
            "idle_webview" => if let Some(v) = b { c.idle_webview = v },
            "idle_hide_ui" => if let Some(v) = b { c.idle_hide_ui = v },
            "destroy_float" => if let Some(v) = b { c.destroy_float = v },
            _ => {}
        }
    }
    match key.as_str() {
        "skin" => if let Some(v) = value.as_str() { set_skin(&app, v) },
        "scale" => if let Some(v) = n { set_float_scale(&app, v) },
        "format" => {   // ajusta la ventana al formato; al salir de la isla vuelve a donde estaba el flotante
            let cur = app.state::<AppState>().settings.lock().unwrap().scale;
            set_float_scale(&app, cur);
            if value.as_str() != Some("island") { if let Some(w) = app.get_webview_window("float") { restore_float_pos(&app, &w); } }
        }
        "source" => if let Some(v) = value.as_str() { set_source(&app, v) },
        "float_on_top" => if let (Some(w), Some(v)) = (app.get_webview_window("float"), b) { let _ = w.set_always_on_top(v); },
        "glass" | "glass_bg" | "glass_color" => { let g = glass_spec(&app.state::<AppState>().settings.lock().unwrap()); js(&app, &format!("window.__lm&&window.__lm.glass({g})")); paint_caption(&app) }
        "theme" => paint_caption(&app),
        "audio_only" => if let Some(v) = b { js(&app, &format!("window.__lm&&window.__lm.audioOnly&&window.__lm.audioOnly({v})")) },
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
        a if a.starts_with("sleep") => { if let Ok(m) = a[5..].parse() { set_sleep(&app, m) } }
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
            // cada ajuste con efecto inmediato pasa por set_setting: la página, la barra y el flotante se enteran ya
            let d = Settings::default();
            for (k, v) in [("skin", serde_json::json!(d.skin)), ("format", serde_json::json!(d.format)), ("float_on_top", serde_json::json!(d.float_on_top)),
                           ("glass", serde_json::json!(d.glass)), ("audio_only", serde_json::json!(d.audio_only)), ("theme", serde_json::json!(d.theme))] {
                set_setting(app.clone(), k.into(), v);
            }
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
    paint_caption(app);
}

// Barra de título nativa de Configuración: con Cristal toma el color de su fondo; si no, sigue el tema.
#[cfg(windows)]
fn paint_caption(app: &AppHandle) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let Some(w) = app.get_webview_window("settings") else { return };
    let Ok(hwnd) = w.hwnd() else { return };
    let st = app.state::<AppState>();
    let (g, theme) = { let c = st.settings.lock().unwrap(); (glass_spec(&c), c.theme.clone()) };
    let dark = match g["tone"].as_str() {
        Some(t) => t == "dark",
        None => theme == "dark" || (theme == "auto" && w.theme().map(|t| t == tauri::Theme::Dark).unwrap_or(true)),
    };
    // color aproximado de la parte de arriba de la página (fondo con el velo de theme.css); 0xFFFFFFFF = el de Windows
    let (caption, text): (u32, u32) = match g["tint"].as_str().and_then(|t| u32::from_str_radix(t.trim_start_matches('#'), 16).ok()) {
        Some(rgb) => {
            let (veil, a) = if dark { (36.0, 0.28) } else { (255.0, 0.42) };
            let ch = |sh: u32| (((rgb >> sh) & 255) as f64 * (1.0 - a) + veil * a) as u32;
            (ch(16) | ch(8) << 8 | ch(0) << 16, if dark { 0x00FFFFFF } else { 0x001A1A1F })   // COLORREF es 0x00BBGGRR
        }
        None => (0xFFFFFFFF, 0xFFFFFFFF),
    };
    let dark = dark as i32;
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &caption as *const _ as _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &text as *const _ as _, 4);
    }
}
#[cfg(not(windows))]
fn paint_caption(_: &AppHandle) {}

// La página de música (YouTube Music / Spotify) y la ventana que la contiene junto con la barra de arriba.
fn main_window(app: &AppHandle) -> Option<Webview> {
    app.get_webview("main")
}
fn main_win(app: &AppHandle) -> Option<Window> {
    app.get_window("main")
}

const BAR_H: f64 = 40.0; // alto de la barra integrada (título, servicio, botones)

// Ubica la barra arriba y la página de música debajo, a todo el ancho (al abrir y cada vez que cambia el tamaño).
fn layout(app: &AppHandle) {
    let (Some(win), Some(main), Some(bar)) = (main_win(app), main_window(app), app.get_webview("bar")) else { return };
    let (Ok(sz), Ok(sf)) = (win.inner_size(), win.scale_factor()) else { return };
    let s = sz.to_logical::<f64>(sf);
    if s.width < 1.0 || s.height <= BAR_H {
        return; // minimizada
    }
    let _ = bar.set_position(LogicalPosition::new(0.0, 0.0));
    let _ = bar.set_size(LogicalSize::new(s.width, BAR_H));
    let _ = main.set_position(LogicalPosition::new(0.0, BAR_H));
    let _ = main.set_size(LogicalSize::new(s.width, s.height - BAR_H));
}

// WebView2: con la ventana oculta, la marca como no visible y pide un uso de memoria bajo (como una pestaña en segundo plano).
#[cfg(windows)]
fn set_webview_idle(w: &Webview, idle: bool) {
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
fn set_webview_idle(_w: &Webview, _idle: bool) {}

// Volver a ocultar solo si la ventana sigue oculta: se comprueba en el hilo principal, el mismo que la muestra.
#[cfg(windows)]
fn set_webview_visible(w: &Webview, visible: bool) {
    let win = w.window();
    let _ = w.with_webview(move |pw| unsafe {
        if visible || !win.is_visible().unwrap_or(true) || MINIMIZED.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = pw.controller().SetIsVisible(visible);
        }
    });
}
#[cfg(not(windows))]
fn set_webview_visible(_w: &Webview, _visible: bool) {}

// Solo el nivel de memoria (sin tocar la visibilidad): sirve para "re-apretar" la memoria tras actividad.
#[cfg(windows)]
fn set_webview_memory(w: &Webview, low: bool) {
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
fn set_webview_memory(_w: &Webview, _low: bool) {}


// Ventana principal minimizada (se trata igual que oculta en la bandeja).
static MINIMIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// Si muere el proceso de la página (falta de memoria, fallo de WebView2) se recarga al momento: antes quedaba en blanco
// y, con la ventana oculta, la música no volvía hasta abrirla. Si muere el navegador entero, la app se reinicia.
#[cfg(windows)]
fn reload_on_crash(app: &AppHandle, w: &Webview) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    };
    let a = app.clone();
    let _ = w.with_webview(move |pw| unsafe {
        let Ok(core) = pw.controller().CoreWebView2() else { return };
        let h = webview2_com::ProcessFailedEventHandler::create(Box::new(move |sender, args| {
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(-1);
            if let Some(args) = args {
                let _ = args.ProcessFailedKind(&mut kind);
            }
            if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                // Fuera del hilo principal: así se cierran bien los plugins (instancia única) antes de relanzar.
                let a2 = a.clone();
                std::thread::spawn(move || a2.restart());
            } else if let (true, Some(core)) = (kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED, sender) {
                let _ = core.Reload();
            }
            Ok(())
        }));
        let mut token = 0i64;
        let _ = core.add_ProcessFailed(&h, &mut token);
    });
}
#[cfg(not(windows))]
fn reload_on_crash(_app: &AppHandle, _w: &Webview) {}

// Llama a métodos del protocolo de depuración de WebView2 (solo para ahorrar memoria; no abre ninguna herramienta).
#[cfg(windows)]
fn cdp(w: &Webview, methods: &'static [&'static str]) {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows_core::HSTRING;
    let _ = w.with_webview(move |pw| unsafe {
        if let Ok(core) = pw.controller().CoreWebView2() {
            for m in methods {
                let h = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(())));
                let _ = core.CallDevToolsProtocolMethod(&HSTRING::from(*m), &HSTRING::from("{}"), &h);
            }
        }
    });
}
#[cfg(not(windows))]
fn cdp(_w: &Webview, _methods: &'static [&'static str]) {}

// Detiene los service workers (procesos de fondo de la página, p. ej. el de YouTube Music que queda vivo tras cambiar
// a Spotify: ~130 MB). Se reinician solos si la página que está abierta los necesita.
fn stop_service_workers(w: &Webview) {
    cdp(w, &["ServiceWorker.enable", "ServiceWorker.stopAllWorkers", "ServiceWorker.disable"]);
}

// Entra o sale del modo ahorro de la ventana principal según los ajustes.
fn set_main_idle(app: &AppHandle, idle: bool) {
    let Some(w) = main_window(app) else { return };
    let st = app.state::<AppState>();
    let (webview, ui) = {
        let c = st.settings.lock().unwrap();
        (c.idle_webview, c.idle_hide_ui)
    };
    let _ = w.eval(&format!("window.__lm&&window.__lm.setHidden({}, {})", idle, idle && ui));
    if webview || !idle {
        set_webview_idle(&w, idle);
    }
    // la barra también deja de consultar el estado mientras no se ve
    if let Some(bar) = app.get_webview("bar") {
        set_webview_idle(&bar, idle);
    }
}

fn show(app: &AppHandle) {
    if let Some(w) = main_win(app) {
        set_main_idle(app, false);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn hide(app: &AppHandle, w: &Window) {
    let _ = w.hide();
    set_main_idle(app, true);
}

fn js(app: &AppHandle, code: &str) {
    if let Some(w) = main_window(app) {
        let _ = w.eval(code);
    }
}

async fn tokio_sleep(secs: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_secs(secs))).await;
}

// Búsqueda rápida: ventana chica que se abre con la lupa del flotante o con Ctrl+Mayús+Espacio.
// La página de música hace la búsqueda (inject.js: __lm.search) y devuelve los resultados con search_results.
fn toggle_search(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("search") {
        if w.is_visible().unwrap_or(false) { let _ = w.hide(); close_search_later(&w); return; }
        place_search(&w);
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.eval("window.lmOpen&&lmOpen()");
        return;
    }
    #[allow(unused_mut)]
    let mut b = WebviewWindowBuilder::new(app, "search", WebviewUrl::App("search.html".into()))
        .title("Lite Music Búsqueda")
        .inner_size(556.0, 470.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false);
    #[cfg(windows)]
    {
        b = b.additional_browser_args(BROWSER_ARGS);
    }
    let Ok(w) = b.build() else { return };
    place_search(&w);
    let ww = w.clone();
    w.on_window_event(move |e| if let WindowEvent::Focused(false) = e { let _ = ww.hide(); close_search_later(&ww); });   // al hacer clic afuera se cierra
    let _ = w.show();
    let _ = w.set_focus();
}

// Crear una ventana desde un comando sincrónico (la lupa del flotante) traba Tauri en Windows:
// la búsqueda siempre se abre desde una tarea aparte.
fn open_search(app: &AppHandle) {
    let a = app.clone();
    tauri::async_runtime::spawn(async move { toggle_search(&a) });
}

// centrada a lo ancho y un poco arriba, en la pantalla donde está el mouse
fn place_search(w: &WebviewWindow) {
    let m = w.cursor_position().ok().and_then(|p| w.monitor_from_point(p.x, p.y).ok().flatten()).or_else(|| w.primary_monitor().ok().flatten());
    if let (Some(m), Ok(sz)) = (m, w.outer_size()) {
        let (mp, ms) = (m.position(), m.size());
        let _ = w.set_position(tauri::PhysicalPosition::new(mp.x + (ms.width as i32 - sz.width as i32) / 2, mp.y + ms.height as i32 * 16 / 100));
    }
}

#[tauri::command]
async fn search(app: AppHandle, q: String, kind: String) -> serde_json::Value {
    let seq = { let st = app.state::<AppState>(); let mut s = st.search.lock().unwrap(); s.0 += 1; s.1 = None; s.0 };
    js(&app, &format!("window.__lm&&window.__lm.search&&window.__lm.search({},{},{seq})", serde_json::json!(q), serde_json::json!(kind)));
    for _ in 0..80 {   // hasta 8 s: Spotify carga su página de búsqueda
        let _ = tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_millis(100))).await;
        let st = app.state::<AppState>();
        let s = st.search.lock().unwrap();
        if s.0 != seq { return serde_json::json!({ "stale": true }); }   // ya se escribió otra cosa
        if let Some(v) = &s.1 { return v.clone(); }
    }
    serde_json::json!({ "items": [], "error": "La página no respondió" })
}

#[tauri::command]
fn search_results(app: AppHandle, seq: u64, items: serde_json::Value, error: String) {
    let st = app.state::<AppState>();
    let mut s = st.search.lock().unwrap();
    if s.0 == seq { s.1 = Some(serde_json::json!({ "items": items, "error": error })); }
}

// Oculta un minuto, la ventana de búsqueda se cierra: libera su proceso (~22 MB). Abrirla de nuevo tarda un instante.
fn close_search_later(w: &WebviewWindow) {
    let w = w.clone();
    tauri::async_runtime::spawn(async move {
        tokio_sleep(60).await;
        if !w.is_visible().unwrap_or(true) { let _ = w.destroy(); }
    });
}

#[tauri::command]
fn search_play(app: AppHandle, i: u64, open: bool) {
    js(&app, &format!("window.__lm&&window.__lm.searchPlay&&window.__lm.searchPlay({i})"));
    if let Some(w) = app.get_webview_window("search") { let _ = w.hide(); close_search_later(&w); }
    if open { show(&app); }   // un artista se abre en la ventana
}

#[tauri::command]
fn search_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window("search") { let _ = w.hide(); close_search_later(&w); }
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
        "sz80" => set_float_scale(app, 0.8),
        "sz100" => set_float_scale(app, 1.0),
        "sz125" => set_float_scale(app, 1.25),
        "dog" => set_skin(app, "dog"),
        "cat" => set_skin(app, "cat"),
        "cover" => set_skin(app, "cover"),
        "none" => set_skin(app, "none"),
        id @ ("trabajando" | "programando" | "tradeando" | "descansando" | "estudiando" | "festejando") => set_skin(app, id),
        "pomo25" => start_pomo(app, 25, 5),
        "pomo50" => start_pomo(app, 50, 10),
        "pomo0" => stop_pomo(app),
        id if id.starts_with("sleep") => { if let Ok(m) = id[5..].parse() { set_sleep(app, m) } } // "sleep30" = en 30 min, "sleep0" = cancelar
        "cfg" => open_settings(app),
        "upd" => {
            let a = app.clone();
            if update_pending() {
                tauri::async_runtime::spawn(install_update(a));
            } else {
                tauri::async_runtime::spawn(check_update(a, true));
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

const HOTKEY_MODS: tauri_plugin_global_shortcut::Modifiers = tauri_plugin_global_shortcut::Modifiers::CONTROL
    .union(tauri_plugin_global_shortcut::Modifiers::ALT)
    .union(tauri_plugin_global_shortcut::Modifiers::SHIFT);

const SEARCH_MODS: tauri_plugin_global_shortcut::Modifiers = tauri_plugin_global_shortcut::Modifiers::CONTROL
    .union(tauri_plugin_global_shortcut::Modifiers::SHIFT);

fn on_hotkey(app: &AppHandle, sc: &tauri_plugin_global_shortcut::Shortcut) {
    use tauri_plugin_global_shortcut::Code;
    if sc.matches(SEARCH_MODS, Code::Space) { return open_search(app); }
    for (code, action, step) in [(Code::Space, "playpause", 0.0), (Code::ArrowRight, "next", 0.0), (Code::ArrowLeft, "prev", 0.0),
                                 (Code::ArrowUp, "volume_step", 0.1), (Code::ArrowDown, "volume_step", -0.1)] {
        if sc.matches(HOTKEY_MODS, code) {
            control(app.clone(), action.into(), Some(step));
        }
    }
}

mod thumbbar;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![report_state, get_state, control, scale_float, get_settings, set_setting, settings_action, choose_source, open_link, bar_state, bar_action, search, search_results, search_play, search_hide])
        .manage(AppState { pomo: Mutex::new(Pomo { phase: Phase::Off, end: Instant::now(), focus_s: 1500, rest_s: 300, prev_skin: String::new() }), user_float: Mutex::new(true), auto_hidden: Mutex::new(false), settings: Mutex::new(Settings::default()), pending_pos: Mutex::new(None), player: Mutex::new(PlayerState::default()), float_visible: Mutex::new(true), last_report: Mutex::new(Instant::now()), pos_at: Mutex::new(Instant::now()), sleep_at: Mutex::new(None), search: Mutex::new((0, None)) })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app, sc, ev| {
            if ev.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                on_hotkey(app, sc);
            }
        }).build())
        .setup(|app| {
            *app.state::<AppState>().settings.lock().unwrap() = load_settings(app.handle());
            let st = app.state::<AppState>();
            let (start_url, glass, audio_only) = { let c = st.settings.lock().unwrap(); (source_url(&c.source), glass_spec(&c), c.audio_only) };
            // estilo Cristal: las hojas de cada sitio viajan dentro del script; si arranca prendido se aplica antes de pintar
            let init = include_str!("inject.js")
                .replace("/*__LM_GLASS_CSS__*/{}", &serde_json::json!({ "ytm": concat!(include_str!("glass-ytm.css"), include_str!("glass-ytm-pages.css")), "sp": concat!(include_str!("glass-sp.css"), include_str!("glass-sp-pages.css")) }).to_string())
                .replace("/*__LM_GLASS_ON__*/null", &glass.to_string())
                .replace("/*__LM_AUDIO_ONLY__*/true", &audio_only.to_string());
            #[allow(unused_mut)]
            let mut builder = WebviewBuilder::new("main", WebviewUrl::External(start_url.parse().unwrap()))
            .user_agent(&user_agent())
            .initialization_script(&init)
            .on_navigation(|u| {
                let ok = allowed_in_app(u);
                if !ok {
                    open_in_browser(u.as_str());
                }
                ok
            })
            // Una página que se recarga con la ventana oculta (caída, vigilante, cambio de servicio) vuelve al modo ahorro.
            .on_page_load(|w, p| {
                if p.event() != tauri::webview::PageLoadEvent::Finished { return; }
                // el script se armó al abrir la app: si el Cristal cambió desde entonces, aquí se corrige
                let st = w.app_handle().state::<AppState>();
                let (glass, audio_only) = { let c = st.settings.lock().unwrap(); (glass_spec(&c), c.audio_only) };
                let _ = w.eval(&format!("window.__lm&&window.__lm.glass({glass});window.__lm&&window.__lm.audioOnly&&window.__lm.audioOnly({audio_only})"));
                if !w.window().is_visible().unwrap_or(true) || MINIMIZED.load(std::sync::atomic::Ordering::SeqCst) {
                    set_main_idle(w.app_handle(), true);
                }
            })
            .on_new_window(|u, _| {
                if allowed_in_app(&u) {
                    tauri::webview::NewWindowResponse::Allow
                } else {
                    open_in_browser(u.as_str());
                    tauri::webview::NewWindowResponse::Deny
                }
            });

            // Menos procesos y sin servicios de red/actualización de fondo de WebView2.
            // Barra propia arriba (título, servicio, botones y controles de ventana): reemplaza el marco y el menú de Windows.
            #[allow(unused_mut)]
            let mut bar = WebviewBuilder::new("bar", WebviewUrl::App("titlebar.html".into()));
            #[cfg(windows)]
            {
                builder = builder.additional_browser_args(BROWSER_ARGS);
                bar = bar.additional_browser_args(BROWSER_ARGS);
            }

            let win = WindowBuilder::new(app, "main")
                .title("Lite Music")
                .visible(app.state::<AppState>().settings.lock().unwrap().welcomed)
                .inner_size(1100.0, 760.0)
                .min_inner_size(460.0, 540.0)
                .decorations(false)
                .shadow(true)
                .build()?;
            let sz = win.inner_size()?.to_logical::<f64>(win.scale_factor()?);
            win.add_child(bar, LogicalPosition::new(0.0, 0.0), LogicalSize::new(sz.width, BAR_H))?;
            let window = win.add_child(builder, LogicalPosition::new(0.0, BAR_H), LogicalSize::new(sz.width, sz.height - BAR_H))?;
            reload_on_crash(app.handle(), &window);
            if !app.state::<AppState>().settings.lock().unwrap().welcomed {
                open_welcome(app.handle());
            }
            // Arranque con Windows: empieza oculta en la bandeja.
            if std::env::args().any(|a| a == "--minimized") {
                hide(app.handle(), &win);
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
            let w = win.clone();
            let wapp = app.handle().clone();
            win.on_window_event(move |e| match e {
                WindowEvent::CloseRequested { api, .. } => {
                    if wapp.state::<AppState>().settings.lock().unwrap().close_to_tray {
                        api.prevent_close();
                        hide(&wapp, &w);
                    } else {
                        wapp.exit(0);
                    }
                }
                // Minimizada ahorra igual que en la bandeja (antes seguía dibujando la página como si se viera).
                WindowEvent::ScaleFactorChanged { .. } => layout(&wapp),
                WindowEvent::Resized(_) => {
                    layout(&wapp);
                    let min = w.is_minimized().unwrap_or(false);
                    if min != MINIMIZED.swap(min, std::sync::atomic::Ordering::SeqCst) {
                        set_main_idle(&wapp, min);
                    }
                }
                _ => {}
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
            let s80 = MenuItem::with_id(app, "sz80", "Pequeño (80%)", true, None::<&str>)?;
            let s100 = MenuItem::with_id(app, "sz100", "Normal (100%)", true, None::<&str>)?;
            let s125 = MenuItem::with_id(app, "sz125", "Grande (125%)", true, None::<&str>)?;
            let m_size = Submenu::with_items(app, "Tamaño del flotante", true, &[&s80, &s100, &s125])?;
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
            let sl = |id: &str, t: &str| MenuItem::with_id(app, id, t, true, None::<&str>);
            let m_sleep = Submenu::with_items(app, "Temporizador para dormir", true, &[&sl("sleep15", "En 15 min")?, &sl("sleep30", "En 30 min")?,
                &sl("sleep60", "En 1 hora")?, &sl("sleep90", "En 1 h 30 min")?, &PredefinedMenuItem::separator(app)?, &sl("sleep0", "Cancelar")?])?;
            let m_quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&m_show, &sep, &m_pp, &m_prev, &m_next, &sep, &m_src, &m_float, &m_skin, &m_size, &m_pomo, &m_sleep, &sep, &m_cfg, &m_upd, &sep, &m_quit])?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Lite Music")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| on_menu(app, ev.id.as_ref()))
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button, button_state: MouseButtonState::Up, .. } = ev {
                        match button {
                            MouseButton::Left => show(tray.app_handle()),
                            MouseButton::Middle => js(tray.app_handle(), "window.__lm&&window.__lm.playPause()"),
                            _ => {}
                        }
                    }
                })
                .build(app)?;

            let _ = UPD.set(Mutex::new(UpdUi { tray_item: m_upd_tray, version: None }));

            // Atajos globales: Ctrl+Alt+Mayús + Espacio / → / ← / ↑ / ↓. Con Mayús para no chocar con AltGr ni con
            // Ctrl+Alt+flechas (girar la pantalla en algunos equipos). Si otra app ya usa uno, ese queda sin registrar.
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut};
                for code in [Code::Space, Code::ArrowRight, Code::ArrowLeft, Code::ArrowUp, Code::ArrowDown] {
                    let _ = app.global_shortcut().register(Shortcut::new(Some(HOTKEY_MODS), code));
                }
                let _ = app.global_shortcut().register(Shortcut::new(Some(SEARCH_MODS), Code::Space));   // búsqueda rápida
            }
            thumbbar::install(app.handle(), &win);

            start_ticker(app.handle().clone());
            island_hover(app.handle().clone());

            // Búsqueda de versiones nuevas (solo avisa): al abrir, tras unos segundos, y luego cada hora. Si falla
            // (p. ej. todavía sin internet al arrancar Windows) se reintenta a los 5 minutos en vez de esperar la hora.
            let a = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio_sleep(8).await;
                loop {
                    let ok = !a.state::<AppState>().settings.lock().unwrap().auto_update || check_update(a.clone(), false).await;
                    tokio_sleep(if ok { 3600 } else { 300 }).await;
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al iniciar Lite Music");
}

#[cfg(test)]
mod tests {
    #[test]
    fn el_tono_del_cristal_sale_del_fondo() {
        let mut c = super::Settings::default();
        assert!(super::glass_spec(&c).is_null());
        c.glass = "screen".into();
        assert_eq!(super::glass_spec(&c)["tone"], "dark");
        c.glass_bg = "light".into();
        assert_eq!(super::glass_spec(&c)["tone"], "light");
        c.glass_bg = "custom".into(); c.glass_color = "#ffe08a".into();
        assert_eq!(super::glass_spec(&c), serde_json::json!({ "tone": "light", "tint": "#ffe08a" }));
    }

    #[test]
    fn settings_viejos_migran_la_escala() {
        let viejo: super::Settings = serde_json::from_str(r#"{"scale": 0.6, "welcomed": true}"#).unwrap();
        assert!(!viejo.float_v2 && viejo.scale == 0.6 && viejo.theme == "auto");
        assert!(super::Settings::default().float_v2);
        // la migración y la marca BOM pasan por la misma lectura que usa la app
        let s = super::parse_settings("\u{feff}{\"scale\": 0.6, \"welcomed\": true, \"glass\": \"art\"}").expect("con BOM se lee igual");
        assert!(s.welcomed && s.glass == "art" && s.scale == 1.0 && s.float_v2);
        assert!(super::parse_settings("{roto").is_none());
    }

    #[test]
    fn allowed_in_app() {
        let ok = |s: &str| super::allowed_in_app(&s.parse().unwrap());
        for s in ["https://music.youtube.com/watch?v=1", "https://accounts.google.com/x", "https://accounts.google.com.ar/x",
                  "https://accounts.google.co.uk/x", "https://www.google.de/x", "https://accounts.spotify.com/x", "about:blank"] {
            assert!(ok(s), "{s}");
        }
        for s in ["http://music.youtube.com/", "https://evil.com/", "https://google.evil.com/", "https://notyoutube.com/",
                  "https://youtube.com.evil.io/", "https://google.com.evil.io/", "file:///C:/x"] {
            assert!(!ok(s), "{s}");
        }
    }
}
