// Botones Anterior / Reproducir-Pausar / Siguiente en la miniatura de la barra de tareas (como el reproductor de Windows).
// Todo corre en el hilo principal, donde vive la ventana: el estado se guarda ahí y no hay hilos ni temporizadores.
#[cfg(windows)]
mod imp {
    use std::{cell::RefCell, sync::OnceLock};
    use tauri::{AppHandle, Manager, WebviewWindow};
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::CreateBitmap;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{
        DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THBF_ENABLED, THBN_CLICKED, THB_FLAGS, THB_ICON, THB_TOOLTIP,
        THUMBBUTTON,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, RegisterWindowMessageW, HICON, ICONINFO, WM_COMMAND};

    const ID0: u32 = 0x4C01; // ids altos: no se confunden con los del menú de la ventana
    const ACTIONS: [&str; 3] = ["prev", "pp", "next"];

    struct Bar {
        hwnd: HWND,
        list: Option<ITaskbarList3>,
        icons: [HICON; 4], // anterior, reproducir, pausar, siguiente
        playing: bool,
    }
    thread_local! { static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) }; }
    static APP: OnceLock<AppHandle> = OnceLock::new();
    static CREATED: OnceLock<u32> = OnceLock::new(); // "TaskbarButtonCreated": el botón se (re)creó, hay que volver a agregarlos

    // Ícono blanco de 16x16 a partir de una forma (true = píxel pintado).
    fn icon(shape: impl Fn(i32, i32) -> bool) -> HICON {
        let mut px = [0u32; 256];
        for y in 0..16 {
            for x in 0..16 {
                if shape(x, y) {
                    px[(y * 16 + x) as usize] = 0xFFFF_FFFF;
                }
            }
        }
        unsafe {
            let color = CreateBitmap(16, 16, 1, 32, Some(px.as_ptr().cast()));
            let mask = CreateBitmap(16, 16, 1, 1, Some([0u8; 32].as_ptr().cast()));
            CreateIconIndirect(&ICONINFO { fIcon: true.into(), hbmMask: mask, hbmColor: color, ..Default::default() }).unwrap_or_default()
        }
    }
    // Triángulo hacia la derecha entre x0 y x0+6 (filas 3 a 12).
    fn tri(x: i32, y: i32, x0: i32) -> bool {
        (3..=12).contains(&y) && x >= x0 && x - x0 <= (y - 3).min(12 - y) * 3 / 2
    }

    fn button(i: u32, icon: HICON, tip: &str) -> THUMBBUTTON {
        let mut b = THUMBBUTTON { dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS, iId: ID0 + i, hIcon: icon, dwFlags: THBF_ENABLED, ..Default::default() };
        for (d, c) in b.szTip.iter_mut().zip(tip.encode_utf16().take(259)) {
            *d = c;
        }
        b
    }
    fn pp(bar: &Bar) -> THUMBBUTTON {
        if bar.playing { button(1, bar.icons[2], "Pausar") } else { button(1, bar.icons[1], "Reproducir") }
    }

    fn add_buttons() {
        BAR.with(|b| {
            let mut b = b.borrow_mut();
            let Some(bar) = b.as_mut() else { return };
            unsafe {
                let Ok(list) = CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER) else { return };
                if list.HrInit().is_ok()
                    && list.ThumbBarAddButtons(bar.hwnd, &[button(0, bar.icons[0], "Anterior"), pp(bar), button(2, bar.icons[3], "Siguiente")]).is_ok()
                {
                    bar.list = Some(list);
                }
            }
        });
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if Some(&msg) == CREATED.get() {
            add_buttons();
        } else if msg == WM_COMMAND && (wp.0 >> 16) as u32 == THBN_CLICKED {
            let i = (wp.0 & 0xFFFF) as u32;
            if let (Some(a), Some(app)) = (i.checked_sub(ID0).and_then(|i| ACTIONS.get(i as usize)), APP.get()) {
                crate::on_menu(app, a);
                return LRESULT(0);
            }
        }
        unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
    }

    pub fn install(app: &AppHandle, w: &WebviewWindow) {
        let Ok(h) = w.hwnd() else { return };
        let _ = APP.set(app.clone());
        let hwnd = HWND(h.0 as _);
        let skip = |d: i32| move |x: i32, y: i32| { let x = if d > 0 { x } else { 15 - x }; tri(x, y, 3) || ((11..=12).contains(&x) && (3..=12).contains(&y)) };
        let icons = [
            icon(skip(-1)),
            icon(|x, y| tri(x, y, 5)),
            icon(|x, y| ((4..=6).contains(&x) || (9..=11).contains(&x)) && (3..=12).contains(&y)),
            icon(skip(1)),
        ];
        let playing = app.state::<crate::AppState>().player.lock().unwrap().playing;
        BAR.with(|b| *b.borrow_mut() = Some(Bar { hwnd, list: None, icons, playing }));
        unsafe {
            let _ = CREATED.set(RegisterWindowMessageW(w!("TaskbarButtonCreated")));
            let _ = SetWindowSubclass(hwnd, Some(proc), 0x4C4D, 0);
        }
        add_buttons(); // por si el botón de la barra de tareas ya existía
    }

    // Cambia el ícono central (reproducir / pausar). Se llama desde cualquier hilo.
    pub fn update(app: &AppHandle, playing: bool) {
        let _ = app.run_on_main_thread(move || {
            BAR.with(|b| {
                let mut b = b.borrow_mut();
                let Some(bar) = b.as_mut() else { return };
                bar.playing = playing;
                if let Some(list) = &bar.list {
                    let _ = unsafe { list.ThumbBarUpdateButtons(bar.hwnd, &[pp(bar)]) };
                }
            })
        });
    }
}

#[cfg(windows)]
pub use imp::{install, update};

#[cfg(not(windows))]
pub fn install(_app: &tauri::AppHandle, _w: &tauri::WebviewWindow) {}
#[cfg(not(windows))]
pub fn update(_app: &tauri::AppHandle, _playing: bool) {}
