//! DX-1d: abre el chat de la app real (por el camino del atajo) y comprueba
//! que la app sigue viva.
//!
//! La app se cayó al abrir el chat (Windows: `0xc0000409`, el aborto de Rust).
//! Este ejemplo monta la app **real** (`VoiceApp`, la barra y el panel del
//! chat con su ventana nativa) con un historial **sintético** en una carpeta
//! temporal, abre el chat por el mismo camino que el atajo (`handle_hotkey
//! ("chat")` → `toggle_chat` → `ChatPanel::open`) y espera diez segundos.
//!
//! - No hay micrófono: la captura es de prueba (`MicCapture::silent`).
//! - No hay altavoz: el `SpeakerApi` es mudo.
//! - No hay atajos globales, ni enjambre, ni agente: `background = false`.
//! - La ventana del chat sí es real (por eso el camino del `WndProc` se
//!   ejercita de verdad).
//!
//! ```text
//! cargo run --release -p ez-app --example diag_chat_smoke
//! cargo run --release -p ez-app --example diag_chat_smoke -- chat
//! ```
//!
//! El modo por omisión reproduce la secuencia que tumbó la app: llamada
//! iniciada, «HUD: panel» (el menú de ajustes), tres segundos, «HUD: chat».
//! `chat` abre el chat directamente (la prueba de humo corta).
//!
//! `big` abre un historial sintético de 320 mensajes (markdown, código,
//! emojis y mensajes vacíos) y lo recorre con la rueda hasta arriba y hasta
//! abajo (DX-1c): la app tiene que seguir viva, no puede haber ninguna línea
//! `PÁNICO` en el registro temporal y el pico de memoria no puede pasar de
//! 1 GB.
//!
//! Si el proceso se cae, el ejemplo muere con él: eso es la reproducción.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_app::app::core::{AppParts, SpeakerApi, VoiceApp};
use ez_app::app::live::LiveHooks;
use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_chat::store::ChatStore;
use ez_core::config::VoiceConfig;

/// Segundos que el chat tiene que seguir vivo.
const ALIVE_SECS: u64 = 10;

/// Altavoz mudo: el ejemplo no necesita Kokoro ni la DLL de ONNX.
struct SilentSpeaker;

impl SpeakerApi for SilentSpeaker {
    fn synthesize(&self, _: &str) -> Result<(Vec<f32>, u32), String> {
        Ok((Vec::new(), 16_000))
    }

    fn play(
        &self,
        _: &[f32],
        _: u32,
        _: &AtomicBool,
        _: Option<&mut dyn FnMut(f64)>,
        _: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(true)
    }

    fn speak(
        &self,
        _: &str,
        _: &AtomicBool,
        _: Option<&mut dyn FnMut(f64)>,
        _: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(true)
    }
}

/// Un texto largo sin espacios.
fn long_word() -> String {
    "x".repeat(2_000)
}

/// Un bloque de markdown con todo lo que puede romper la maquetación.
fn rich() -> String {
    format!(
        "## Encabezado\n\n\
         Un párrafo con **negrita**, *cursiva*, `código` y un [enlace](https://example.com).\n\n\
         - Lista de primer nivel\n\
         - Lista de segundo nivel\n\n\
         > Una cita.\n\n\
         ```rust\n\
         fn main() {{\n\
         \x20   println!(\"hola\");\n\
         }}\n\
         ```\n\n\
         | Caso | Antes | Después |\n\
         |---|---|---|\n\
         | 10 | 3 ms | 1 ms |\n\n\
         ![una imagen](C:\\no\\existe\\imagen.png)\n\n\
         Emojis: 🦙 🧑‍💻 🇦🇷 ✅ 🎉\n\n\
         {}\n",
        long_word()
    )
}

/// El historial sintético, en el JSONL que escribe la app.
fn history_jsonl() -> String {
    let mut texts: Vec<(&str, String)> = vec![
        ("user", "Revisa la rama `work/DX-1`.".to_string()),
        ("assistant", rich()),
        ("user", String::new()),
        ("assistant", String::new()),
        ("user", long_word()),
        ("assistant", "Una respuesta corta.".to_string()),
        ("system", "Conversación compactada.".to_string()),
        ("user", "Emojis 🦙 y acentos: ñáéíóú ¿? ¡!".to_string()),
        ("assistant", rich()),
    ];
    // Hasta 35 mensajes, como el historial que tenía la persona.
    let mut index = texts.len();
    while texts.len() < 35 {
        let role = match index % 3 {
            0 => "user",
            1 => "assistant",
            _ => "system",
        };
        let text = match index % 4 {
            0 => rich(),
            1 => format!("Mensaje {index} con un poco de texto normal."),
            2 => String::new(),
            _ => long_word(),
        };
        texts.push((role, text));
        index += 1;
    }
    history_from(&texts)
}

/// Historial largo para la prueba de humo de DX-1c: 320 mensajes con todas
/// las formas que puede recibir el panel (markdown, código, emojis, vacíos).
fn big_history_jsonl() -> String {
    let mut texts: Vec<(&str, String)> = Vec::new();
    while texts.len() < 320 {
        let index = texts.len();
        let item = match index % 5 {
            0 => (
                "user",
                format!("Mensaje {index}: revisa `work/DX-1c` y dime qué falta. 🦙"),
            ),
            1 => ("assistant", rich()),
            2 => ("user", String::new()),
            3 => (
                "assistant",
                format!(
                    "## Caso {index}\n\n\
                     Texto con **negrita**, ñáéíóú y emojis 🎉\n\n\
                     ```rust\n\
                     fn caso_{index}() {{}}\n\
                     ```\n"
                ),
            ),
            _ => ("system", "Conversación compactada.".to_string()),
        };
        texts.push(item);
    }
    history_from(&texts)
}

/// El JSONL con la sesión y los mensajes dados.
fn history_from(texts: &[(&str, String)]) -> String {
    let mut out = String::new();
    out.push_str("{\"v\":1,\"t\":1000.0,\"event\":\"session\",\"session\":1,\"seq\":1}\n");
    for (number, (role, text)) in texts.iter().enumerate() {
        let seq = number as i64 + 2;
        let escaped = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
        out.push_str(&format!(
            "{{\"v\":1,\"t\":{},\"session\":1,\"role\":\"{role}\",\"source\":\"chat\",\"text\":{escaped},\"seq\":{seq}}}\n",
            1000.0 + seq as f64
        ));
    }
    out
}

/// La apariencia de la persona (sin secretos): tema propio, círculo, 85 %.
///
/// Los valores salen de las claves de interfaz que la app guarda en
/// `voice.json`; el historial NO se lee de su carpeta.
fn seed_config(dir: &std::path::Path) -> std::io::Result<()> {
    let config = serde_json::json!({
        "theme": "custom",
        "theme_base": "light",
        "theme_accent": "#FF9EC4",
        "theme_background": "#2E2A6B",
        "theme_primary": "#7A6FF0",
        "theme_secondary": "#C9B8FF",
        "ui_layout": "pill",
        "drop_layout": "pill",
        "ui_palette": "sky",
        "ui_scale": 0.85,
        "drop_dock": "left",
        "chat_width": 434.0,
        "chat_height": 704.0,
        "chat_voice": true
    });
    std::fs::write(
        dir.join("voice.json"),
        serde_json::to_vec_pretty(&config).unwrap_or_default(),
    )
}

/// Escribe el historial sintético en una carpeta temporal y devuelve la ruta.
fn seed_history(dir: &std::path::Path, content: &str) -> std::io::Result<PathBuf> {
    let history = dir.join("history");
    std::fs::create_dir_all(&history)?;
    let path = history.join("chat.jsonl");
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Entrega mensajes de ratón a la ventana del chat de **este** proceso.
///
/// Reproduce lo que Windows hace cuando el panel nace bajo el puntero: mover,
/// pulsar y soltar dentro del campo de texto. No usa `SendInput`, así que no
/// mueve el ratón de la persona ni toca la ventana de su app.
#[cfg(windows)]
fn poke_chat_window() {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetClientRect, GetWindowTextW, GetWindowThreadProcessId,
        PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    };

    struct Info {
        hwnd: isize,
        pid: u32,
        class: String,
        title: String,
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lparam.0 as *mut Vec<Info>) };
        let mut pid = 0u32;
        // SAFETY: `hwnd` lo entrega EnumWindows y `pid` es local.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        let mut class = [0u16; 128];
        // SAFETY: el búfer es local y del tamaño que pide el API.
        let length = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
        let mut title = [0u16; 256];
        // SAFETY: igual que el anterior.
        let shown = unsafe { GetWindowTextW(hwnd, &mut title) }.max(0) as usize;
        list.push(Info {
            hwnd: hwnd.0 as isize,
            pid,
            class: String::from_utf16_lossy(&class[..length.min(class.len())]),
            title: String::from_utf16_lossy(&title[..shown.min(title.len())]),
        });
        BOOL::from(true)
    }

    let mut windows: Vec<Info> = Vec::new();
    // SAFETY: el callback solo escribe en `windows`, vivo durante la llamada.
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut windows as *mut Vec<Info> as isize),
        );
    }
    let me = std::process::id();
    let mut poked = 0;
    for info in windows.iter().filter(|info| {
        info.pid == me && info.class == "ezagent-layered-window" && info.title == "EzAgent"
    }) {
        let hwnd = HWND(info.hwnd as *mut std::ffi::c_void);
        let mut rect = RECT::default();
        // SAFETY: `hwnd` pertenece a este proceso y `rect` es local.
        if unsafe { GetClientRect(hwnd, &mut rect) }.is_err() {
            continue;
        }
        let (width, height) = (rect.right.max(2), rect.bottom.max(2));
        let point = |x: i32, y: i32| LPARAM(((y as isize) << 16) | (x as isize & 0xffff));
        // Moverse por el historial y bajar hasta el campo de texto.
        for (x, y) in [
            (width / 2, height / 2),
            (width / 2, height * 3 / 4),
            (width / 2, height - 80),
            (width / 2, height - 40),
        ] {
            // SAFETY: mensaje propio dirigido a una ventana de este proceso.
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_MOUSEMOVE, WPARAM(0), point(x, y));
            }
        }
        // Pulsar dentro del campo, como el clic que abre el chat bajo el puntero.
        for (x, y) in [(width / 2, height - 80), (width / 2, height - 40)] {
            // SAFETY: mensajes propios; el WndProc los valida.
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_LBUTTONDOWN, WPARAM(1), point(x, y));
                let _ = PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), point(x, y));
            }
        }
        poked += 1;
    }
    eprintln!("monkey: {poked} ventana(s) del chat recibieron ratón");
}

#[cfg(not(windows))]
fn poke_chat_window() {}

/// Entrega muescas de rueda a la ventana del chat de **este** proceso.
///
/// `delta > 0` sube hacia los mensajes viejos, como el ratón. No usa
/// `SendInput`, así que no mueve el ratón de la persona.
#[cfg(windows)]
fn wheel_chat(delta: i16, notches: usize) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, PostMessageW,
        WM_MOUSEWHEEL,
    };

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };
        let mut pid = 0u32;
        // SAFETY: `hwnd` lo entrega EnumWindows y `pid` es local.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid != std::process::id() {
            return BOOL::from(true);
        }
        let mut class = [0u16; 128];
        // SAFETY: búfer local del tamaño que pide el API.
        let length = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
        let mut title = [0u16; 256];
        // SAFETY: igual que el anterior.
        let shown = unsafe { GetWindowTextW(hwnd, &mut title) }.max(0) as usize;
        let class = String::from_utf16_lossy(&class[..length.min(class.len())]);
        let title = String::from_utf16_lossy(&title[..shown.min(title.len())]);
        if class == "ezagent-layered-window" && title == "EzAgent" {
            list.push(hwnd.0 as isize);
        }
        BOOL::from(true)
    }

    let mut windows: Vec<isize> = Vec::new();
    // SAFETY: el callback solo escribe en `windows`, vivo durante la llamada.
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut windows as *mut Vec<isize> as isize),
        );
    }
    let wparam = WPARAM((delta as u16 as usize) << 16);
    for _ in 0..notches {
        for hwnd in &windows {
            let hwnd = HWND(*hwnd as *mut std::ffi::c_void);
            // SAFETY: mensajes propios a una ventana de este proceso.
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_MOUSEWHEEL, wparam, LPARAM(0));
            }
        }
    }
    eprintln!(
        "rueda: {notches} muescas de {delta} en {} ventana(s)",
        windows.len()
    );
}

#[cfg(not(windows))]
fn wheel_chat(_delta: i16, _notches: usize) {}

/// Memoria de trabajo del proceso, en MB.
#[cfg(windows)]
fn working_set_mb() -> u64 {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: `counters` es local y del tamaño declarado.
    if unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) }.is_err() {
        return 0;
    }
    (counters.WorkingSetSize / (1024 * 1024)) as u64
}

#[cfg(not(windows))]
fn working_set_mb() -> u64 {
    0
}

/// Líneas `PÁNICO` que dejó el hook en el registro temporal.
fn panic_lines(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let Ok(content) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in content.lines().filter(|line| line.contains("PÁNICO")) {
            out.push(line.to_string());
        }
    }
    out
}

/// Hace clic en un botón de la barra como lo haría el ratón: los mismos
/// mensajes que entrega Windows a su `WndProc`, sin `SendInput`.
#[cfg(windows)]
fn click_bar(app: &VoiceApp, item: &str) -> bool {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetClientRect, GetWindowTextW, GetWindowThreadProcessId,
        PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    };

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lparam.0 as *mut Vec<(HWND, u32, String, String)>) };
        let mut pid = 0u32;
        // SAFETY: `hwnd` lo entrega EnumWindows y `pid` es local.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        let mut class = [0u16; 128];
        // SAFETY: búfer local del tamaño que pide el API.
        let length = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
        let mut title = [0u16; 256];
        // SAFETY: igual que el anterior.
        let shown = unsafe { GetWindowTextW(hwnd, &mut title) }.max(0) as usize;
        list.push((
            hwnd,
            pid,
            String::from_utf16_lossy(&class[..length.min(class.len())]),
            String::from_utf16_lossy(&title[..shown.min(title.len())]),
        ));
        BOOL::from(true)
    }

    let mut windows: Vec<(HWND, u32, String, String)> = Vec::new();
    // SAFETY: el callback escribe en `windows`, vivo durante la llamada.
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut windows as *mut Vec<(HWND, u32, String, String)> as isize),
        );
    }
    let me = std::process::id();
    let Some((hwnd, ..)) = windows.iter().find(|(_, pid, class, title)| {
        *pid == me && class == "ezagent-layered-window" && title == "drop"
    }) else {
        eprintln!("no encontré la barra");
        return false;
    };
    let hwnd = *hwnd;
    let mut rect = RECT::default();
    // SAFETY: la ventana es de este proceso y `rect` es local.
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_err() {
        return false;
    }
    let bs = app.hud().inner().bs();
    // Los botones solo se muestran con el puntero encima: primero se acerca
    // el ratón a la forma y se deja que la animación los despliegue.
    let center =
        LPARAM((((rect.bottom / 2) as isize) << 16) | (((rect.right / 2) as isize) & 0xffff));
    // SAFETY: mensaje propio a una ventana de este proceso.
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_MOUSEMOVE, WPARAM(0), center);
    }
    std::thread::sleep(Duration::from_millis(700));
    eprintln!(
        "barra: cliente {}x{} bs {bs} centro {:?}",
        rect.right,
        rect.bottom,
        app.hud().inner().item_at(
            f64::from(rect.right / 2) / bs,
            f64::from(rect.bottom / 2) / bs
        )
    );
    let mut target = None;
    'scan: for y in (0..rect.bottom).step_by(2) {
        for x in (0..rect.right).step_by(2) {
            if app
                .hud()
                .inner()
                .item_at(f64::from(x) / bs, f64::from(y) / bs)
                == Some(item)
            {
                target = Some((x, y));
                break 'scan;
            }
        }
    }
    let Some((x, y)) = target else {
        eprintln!("no encontré el botón {item} en la barra");
        return false;
    };
    let point = LPARAM(((y as isize) << 16) | (x as isize & 0xffff));
    // SAFETY: mensajes propios a una ventana de este proceso.
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_MOUSEMOVE, WPARAM(0), point);
        let _ = PostMessageW(Some(hwnd), WM_LBUTTONDOWN, WPARAM(1), point);
        let _ = PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), point);
    }
    eprintln!("clic en {} de la barra ({x},{y})", item);
    true
}

#[cfg(not(windows))]
fn click_bar(_app: &VoiceApp, _item: &str) -> bool {
    false
}

/// Mensajes que atiende el `WndProc` de las ventanas layered, con coordenadas
/// al azar (dentro y fuera del panel).
#[cfg(windows)]
const FUZZ_MESSAGES: [u32; 12] = [
    0x0200, // WM_MOUSEMOVE
    0x0201, // WM_LBUTTONDOWN
    0x0202, // WM_LBUTTONUP
    0x020A, // WM_MOUSEWHEEL
    0x0084, // WM_NCHITTEST
    0x0021, // WM_MOUSEACTIVATE
    0x0102, // WM_CHAR
    0x0100, // WM_KEYDOWN
    0x02E0, // WM_DPICHANGED
    0x0008, // WM_KILLFOCUS
    0x02A3, // WM_MOUSELEAVE
    0x0007, // WM_SETFOCUS
];

#[cfg(windows)]
fn fuzz_windows(rounds: usize) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowThreadProcessId, PostMessageW,
    };

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lparam.0 as *mut Vec<(HWND, String)>) };
        let mut pid = 0u32;
        // SAFETY: `hwnd` lo entrega EnumWindows y `pid` es local.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        let mut class = [0u16; 128];
        // SAFETY: búfer local del tamaño que pide el API.
        let length = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
        if pid == std::process::id()
            && String::from_utf16_lossy(&class[..length.min(class.len())])
                == "ezagent-layered-window"
        {
            list.push((hwnd, String::new()));
        }
        BOOL::from(true)
    }

    let mut windows: Vec<(HWND, String)> = Vec::new();
    // SAFETY: el callback escribe en `windows`, vivo durante la llamada.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut windows as *mut _ as isize));
    }
    // Generador lineal congruente: reproducible y sin dependencias.
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as u32
    };
    for round in 0..rounds {
        for (hwnd, _) in &windows {
            let message = FUZZ_MESSAGES[next() as usize % FUZZ_MESSAGES.len()];
            let x = next() % 4000;
            let y = next() % 3000;
            let lparam = LPARAM(((y as isize) << 16) | (x as isize & 0xffff));
            let wparam = if message == 0x02E0 {
                WPARAM((48 + next() % 550) as usize)
            } else {
                WPARAM(next() as usize)
            };
            // SAFETY: mensajes propios a ventanas de este proceso.
            unsafe {
                let _ = PostMessageW(Some(*hwnd), message, wparam, lparam);
            }
        }
        if round % 10 == 0 {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    eprintln!("fuzz: {rounds} rondas en {} ventana(s)", windows.len());
}

#[cfg(not(windows))]
fn fuzz_windows(_rounds: usize) {}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sequence".to_string());
    let big = mode == "big";
    let temp = std::env::temp_dir().join(format!("ezagent-dx1d-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp);
    // El hook de pánico escribe el mensaje y la ubicación antes de abortar.
    ez_core::panics::install_hook_in(&temp.join("logs"));
    if let Err(error) = seed_config(&temp) {
        eprintln!("No pude escribir la configuración sintética: {error}");
        std::process::exit(1);
    }
    let content = if big {
        big_history_jsonl()
    } else {
        history_jsonl()
    };
    let history = match seed_history(&temp, &content) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("No pude escribir el historial sintético: {error}");
            std::process::exit(1);
        }
    };
    if big {
        eprintln!("historial: 320 mensajes sintéticos");
    }
    // La app lee `%APPDATA%\EzAgent`: con una carpeta temporal no se toca el
    // historial, la configuración ni el registro de la persona.
    std::env::set_var("APPDATA", &temp);

    let cfg = match VoiceConfig::load(Some(&temp.join("voice.json"))) {
        Ok(cfg) => cfg,
        Err(error) => {
            eprintln!("No pude cargar la configuración: {error}");
            std::process::exit(1);
        }
    };

    let store = Arc::new(ChatStore::new(Some(history)));
    let chat = Arc::new(ChatModel::new(Some(store), false));

    let mut parts = AppParts::new(
        Arc::new(SilentSpeaker),
        Arc::new(parking_lot::Mutex::new(MicCapture::silent(
            VadSettings::default(),
        ))),
    );
    // DECISION: sin hilos de fondo, sin atajos globales, sin enjambre, sin
    // agente y sin captura real: el ejemplo solo quiere la barra y el panel
    // del chat (DX-1d).
    parts.background = false;
    parts.build_swarm = false;
    parts.build_agent = false;
    parts.mic_capture = false;
    parts.whisper_ready = true;
    parts.chat = Some(chat);
    parts.config_file = Some(temp.join("voice.json"));

    let app = VoiceApp::with_services(cfg, None, parts);
    app.set_hooks(Arc::new(LiveHooks::new(&app)));
    // DECISION: los botones de la barra solo aparecen con el cursor de verdad
    // encima, y el ejemplo no mueve el ratón de la persona; el modo `always`
    // (pensado para las pruebas) los deja visibles para poder enviarles el clic.
    app.hud().inner().set_buttons("always");
    eprintln!(
        "tema: {} · escala: {} · layout: {}",
        app.hud().inner().theme_name(),
        app.hud().inner().scale(),
        app.hud().inner().layout()
    );

    if mode != "chat" && !big {
        // La app que se cayó estaba en llamada («Llamada iniciada»).
        app.start_call();
        eprintln!("llamada iniciada");
        if mode == "real" {
            // El clic real del engranaje: barra → `WndProc` → `HUD: panel`.
            std::thread::sleep(Duration::from_secs(1));
            click_bar(&app, "gear");
            app.tick();
            eprintln!("clic del panel enviado");
            std::thread::sleep(Duration::from_secs(3));
        } else {
            // «HUD: panel»: el clic del engranaje pasa por la cola real.
            app.on_hud_action("panel");
            app.tick();
            eprintln!("panel abierto");
            std::thread::sleep(Duration::from_secs(3));
        }
    }

    // El camino del atajo: `handle_hotkey("chat")` → `toggle_chat` →
    // `ChatPanel::open` (ventana nativa, hilo de pintura y `WndProc`). En la
    // secuencia, el clic del botón del chat pasa por la cola real (`HUD: chat`).
    if mode == "chat" || big {
        app.handle_hotkey("chat");
    } else if mode == "real" {
        click_bar(&app, "chat");
        app.tick();
    } else {
        app.on_hud_action("chat");
        app.tick();
    }
    let opened = app.hud().chat_is_open();
    eprintln!("chat abierto: {opened}");

    let deadline = Instant::now() + Duration::from_secs(ALIVE_SECS);
    let mut ticks = 0u64;
    let mut poked = false;
    let mut peak_mb = working_set_mb();
    // Etapas del historial grande: dejar que el primer cuadro termine, subir
    // hasta arriba, bajar hasta abajo (DX-1c).
    let mut stage = 0;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        ticks += 1;
        app.tick();
        peak_mb = peak_mb.max(working_set_mb());
        if big {
            match stage {
                0 if ticks >= 40 => {
                    wheel_chat(32_767, 120);
                    eprintln!("desplazamiento: hasta arriba");
                    stage = 1;
                }
                1 if ticks >= 55 => {
                    eprintln!("vivo tras arriba: {}", app.hud().chat_is_open());
                    stage = 2;
                }
                2 if ticks >= 70 => {
                    wheel_chat(-32_767, 120);
                    eprintln!("desplazamiento: hasta abajo");
                    stage = 3;
                }
                3 if ticks >= 85 => {
                    eprintln!("vivo tras abajo: {}", app.hud().chat_is_open());
                    stage = 4;
                }
                _ => {}
            }
        }
        // `monkey`: entrega mensajes de ratón a la ventana del chat como los
        // entrega Windows (el panel nace bajo el puntero). Sin `SendInput`: no
        // se mueve el ratón de la persona ni se toca otra ventana.
        if mode == "fuzz" && ticks.is_multiple_of(20) {
            fuzz_windows(30);
        }
        if (mode == "monkey" || mode == "real") && ticks >= 5 && !poked {
            poked = true;
            poke_chat_window();
        }
        if !app.hud().chat_is_open() {
            eprintln!("el chat se cerró solo tras {ticks} décimas");
            break;
        }
    }
    let alive = app.hud().chat_is_open();
    let panics = panic_lines(&temp.join("logs"));
    eprintln!("RSS pico: {peak_mb} MB");
    if panics.is_empty() {
        eprintln!("registro temporal: sin PÁNICO");
    } else {
        eprintln!("registro temporal: {} línea(s) de PÁNICO", panics.len());
        for line in &panics {
            eprintln!("{line}");
        }
    }
    eprintln!("RESULT chat_open={alive} alive_secs={ALIVE_SECS}");
    app.shutdown();
    if !alive || !panics.is_empty() || peak_mb > 1024 {
        std::process::exit(1);
    }
}
