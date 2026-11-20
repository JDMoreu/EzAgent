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
//! DX-7b: las ventanas se abren **solo** en el monitor secundario. Antes de
//! crear nada se comprueba que la secundaria exista; si no, el humo falla sin
//! abrir ventanas. No hay ruta de prueba hacia la pantalla principal. Al
//! terminar imprime el rectángulo de cada ventana y falla si alguna quedó fuera
//! del monitor pedido.
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
//! `anim` (DX-1f) abre el chat con el clic real de la barra, espera a que el
//! contenido llegue, lo cierra con el clic real del botón de la cabecera y
//! compara los cuadros de las dos animaciones: la apertura tiene que presentar
//! al menos tantos cuadros intermedios como el cierre y empezar casi en cero.
//! Antes el primer cuadro maquetaba todo el historial y el panel aparecía de
//! golpe.
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
use ez_hud::model::chat::view::{BTN, HEADER_H, PAD};
use ez_hud::win::chat::trace::{self, FrameRecord};
use ez_hud::win::chat::{CLOSE_SECS, MARGIN, MORPH_SECS};
use ez_hud::win::monitor::{monitor_rect, rect_inside, Rect, TEST_MONITOR_ENV};

/// Segundos que el chat tiene que seguir vivo.
const ALIVE_SECS: u64 = 10;

/// Escribe una línea del informe (el ejemplo no usa `eprintln!`).
fn say(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{text}");
}

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

/// Historial normal para medir la animación (DX-1f).
///
/// DECISION: sin la palabra de 2 000 caracteres del humo de DX-1d. Cortarla es
/// O(n²) en `split_word` y tarda segundos; aquí lo que se mide es el morph, no
/// el coste del texto.
fn anim_history_jsonl() -> String {
    let mut texts: Vec<(&str, String)> = Vec::new();
    for index in 0..24 {
        let role = if index % 2 == 0 { "user" } else { "assistant" };
        let text = match index % 4 {
            0 => format!("Mensaje {index}: revisa `work/DX-1f`."),
            1 => format!("## Caso {index}\n\nTexto con **negrita** y ñáéíóú. 🎉"),
            2 => format!("Una respuesta corta del asistente ({index})."),
            _ => format!("```rust\nfn caso_{index}() {{}}\n```"),
        };
        texts.push((role, text));
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

/// Rectángulos de las ventanas layered de este proceso (título, rectángulo).
#[cfg(windows)]
fn layered_windows() -> Vec<(String, Rect)> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    };

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` apunta al `Vec` vivo de `layered_windows`.
        let list = unsafe { &mut *(lparam.0 as *mut Vec<(String, RECT)>) };
        let mut pid = 0u32;
        // SAFETY: `hwnd` lo entrega EnumWindows y `pid` es local.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid != std::process::id() {
            return BOOL::from(true);
        }
        let mut class = [0u16; 128];
        // SAFETY: el búfer es local y del tamaño que pide el API.
        let length = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
        if String::from_utf16_lossy(&class[..length.min(class.len())]) != "ezagent-layered-window" {
            return BOOL::from(true);
        }
        let mut title = [0u16; 256];
        // SAFETY: igual que el anterior.
        let shown = unsafe { GetWindowTextW(hwnd, &mut title) }.max(0) as usize;
        let mut rect = RECT::default();
        // SAFETY: `rect` es local y válido durante la consulta.
        if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
            list.push((
                String::from_utf16_lossy(&title[..shown.min(title.len())]),
                rect,
            ));
        }
        BOOL::from(true)
    }

    let mut list: Vec<(String, RECT)> = Vec::new();
    // SAFETY: el callback solo escribe en `list`, vivo durante la llamada.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut _ as isize));
    }
    list.into_iter()
        .map(|(title, rect)| (title, (rect.left, rect.top, rect.right, rect.bottom)))
        .collect()
}

#[cfg(not(windows))]
fn layered_windows() -> Vec<(String, Rect)> {
    Vec::new()
}

/// DX-7: nombre legible de las ventanas layered del HUD.
fn window_label(title: &str) -> &str {
    match title {
        "drop" => "barra",
        "EzAgent" => "chat",
        "card" => "menú",
        "peek" => "vista rápida",
        other => other,
    }
}

/// DX-7: imprime el monitor pedido y dónde quedó cada ventana propia; falla si
/// alguna se salió del monitor.
fn check_monitor() -> bool {
    let monitor = monitor_rect(None, None);
    let requested = std::env::var(TEST_MONITOR_ENV).unwrap_or_default();
    say(&format!("monitor pedido: {requested:?} · rect {monitor:?}"));
    let windows = layered_windows();
    let mut inside = true;
    for (title, rect) in &windows {
        let ok = rect_inside(*rect, monitor);
        inside &= ok;
        say(&format!(
            "ventana {:?}: rect {rect:?} dentro={ok}",
            window_label(title)
        ));
    }
    if windows.is_empty() {
        say("ventanas: ninguna");
        // DECISION: fuera de Windows no hay ventanas Win32 que comprobar.
        return cfg!(not(windows));
    }
    inside
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

/// Hace clic en el botón de cerrar de la cabecera del chat (ventana propia).
///
/// La barra no sirve para cerrar: con el chat abierto su forma está oculta y
/// `pointer_down` ignora el clic (`form_hidden`). El botón de cerrar del panel
/// sí recibe el mismo camino real (`WndProc` → vista → `Action::close`), y su
/// centro se deduce de la geometría del panel (margen de la sombra y escala).
#[cfg(windows)]
fn click_chat_close(app: &VoiceApp) -> bool {
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
        *pid == me && class == "ezagent-layered-window" && title == "EzAgent"
    }) else {
        say("no encontré la ventana del chat");
        return false;
    };
    let hwnd = *hwnd;
    let mut rect = RECT::default();
    // SAFETY: la ventana es de este proceso y `rect` es local.
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_err() {
        return false;
    }
    // El lienzo es el panel más el margen de la sombra, en píxeles físicos.
    let scale = app.hud().inner().scale();
    let margin = (MARGIN * scale).round_ties_even() as i32;
    let logical_w = f64::from((rect.right - 2 * margin).max(1)) / scale;
    let logical_x = logical_w - PAD - BTN / 2.0 + 6.0;
    let x = margin + (logical_x * scale).round_ties_even() as i32;
    let y = margin + (HEADER_H / 2.0 * scale).round_ties_even() as i32;
    let point = LPARAM(((y as isize) << 16) | (x as isize & 0xffff));
    // SAFETY: mensajes propios a una ventana de este proceso.
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_MOUSEMOVE, WPARAM(0), point);
        let _ = PostMessageW(Some(hwnd), WM_LBUTTONDOWN, WPARAM(1), point);
        let _ = PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), point);
    }
    say(&format!(
        "clic en cerrar del chat ({x},{y}) · lienzo {}x{} · escala {scale}",
        rect.right, rect.bottom
    ));
    true
}

#[cfg(not(windows))]
fn click_chat_close(_app: &VoiceApp) -> bool {
    false
}

/// Cuadros intermedios presentados dentro de la duración de la animación.
///
/// Se descartan los extremos (el cuadro inicial casi en 0 y el final en 1):
/// una apertura que salta de 0 a 1 en un cuadro cuenta **cero**. El instante
/// es el de entrega del cuadro (`ready`), no el del reloj de la animación: un
/// cuadro que se compone en p = 0,5 pero se entrega 300 ms después no se ve
/// "dentro" de la animación.
fn middle_frames(frames: &[FrameRecord], seconds: f64) -> usize {
    frames
        .iter()
        .filter(|frame| {
            frame.p > 0.02 && frame.p < 0.98 && frame.ready >= 0.0 && frame.ready <= seconds
        })
        .count()
}

/// Imprime un cuadro por línea, como la tabla del informe.
fn print_frames(frames: &[FrameRecord]) {
    say("fase       t(ms)  listo(ms)      p    tamaño   alfa  contenido");
    for frame in frames {
        say(&format!(
            "{:<8} {:>7.2} {:>9.2} {:>6.3}   {:>4}x{:<4} {:>4}  {}",
            frame.phase,
            frame.elapsed * 1000.0,
            frame.ready * 1000.0,
            frame.p,
            frame.width,
            frame.height,
            frame.alpha,
            if frame.content { "sí" } else { "no" }
        ));
    }
}

/// DX-1f: la apertura tiene que presentar cuadros intermedios como el cierre.
///
/// Antes, el primer cuadro de la apertura maquetaba todo el historial: cuando
/// terminaba, el morph ya había pasado y el panel aparecía de golpe. El cierre
/// no maquetaba (reusaba la capa de contenido) y sí se veía.
fn check_animation(app: &VoiceApp, opened: bool) -> i32 {
    let mut peak_mb = working_set_mb();
    say(&format!(
        "diario: encendido={} cuadros={}",
        trace::enabled(),
        trace::records().len()
    ));
    // 1) Esperar a que el morph termine y el contenido llegue.
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        app.tick();
        peak_mb = peak_mb.max(working_set_mb());
        if trace::records()
            .iter()
            .any(|frame| frame.phase == "open" && frame.content)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let all = trace::records();
    let opening: Vec<FrameRecord> = all
        .iter()
        .copied()
        .filter(|frame| frame.phase == "opening")
        .collect();
    let first_open = all
        .iter()
        .copied()
        .find(|frame| frame.phase == "open")
        .map(|frame| frame.elapsed);
    let content_at = all
        .iter()
        .copied()
        .find(|frame| frame.phase == "open" && frame.content)
        .map(|frame| frame.elapsed);
    // 2) Cerrar con un clic real en el botón de la cabecera.
    trace::clear();
    let clicked = click_chat_close(app);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        app.tick();
        peak_mb = peak_mb.max(working_set_mb());
        if trace::records()
            .iter()
            .any(|frame| frame.phase == "closing" && frame.p <= 0.0)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let closing: Vec<FrameRecord> = trace::records()
        .into_iter()
        .filter(|frame| frame.phase == "closing")
        .collect();

    say("=== cuadros de la apertura ===");
    print_frames(&opening);
    say("=== cuadros del cierre ===");
    print_frames(&closing);

    let open_middle = middle_frames(&opening, MORPH_SECS);
    let close_middle = middle_frames(&closing, CLOSE_SECS);
    let start = opening.first().map_or(1.0, |frame| frame.p);
    say(&format!(
        "apertura: {open_middle} cuadros intermedios · primer p {start:.3} · contenido a los {} ms",
        content_at.map_or(f64::NAN, |seconds| seconds * 1000.0)
    ));
    say(&format!(
        "cierre: {close_middle} cuadros intermedios · primer cuadro abierto a los {} ms",
        first_open.map_or(f64::NAN, |seconds| seconds * 1000.0)
    ));
    let ok = opened
        && clicked
        && start < 0.5
        && open_middle >= close_middle.max(2)
        && close_middle >= 2
        && content_at.is_some()
        && peak_mb <= 1024;
    say(&format!(
        "RESULT anim opening_frames={open_middle} closing_frames={close_middle} first_p={start:.3} content_ms={}",
        content_at.map_or(f64::NAN, |seconds| seconds * 1000.0)
    ));
    say(&format!("RSS pico (anim): {peak_mb} MB"));
    if ok {
        say("estado: la apertura se anima");
        0
    } else {
        say("estado: la apertura es un salto");
        1
    }
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
    // DX-7b: sin secundaria verificada no se crea ninguna ventana.
    std::env::set_var(TEST_MONITOR_ENV, "secondary");
    let monitor = match ez_hud::win::monitor::require_secondary_monitor() {
        Ok(rect) => rect,
        Err(reason) => {
            say(&format!("no se abre el humo: {reason}"));
            std::process::exit(2);
        }
    };
    say(&format!(
        "monitor verificado: secundario · rect {monitor:?}"
    ));
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sequence".to_string());
    let big = mode == "big";
    // DX-1f: `anim` mide la animación de apertura y de cierre.
    let anim = mode == "anim";
    // Carpeta única por corrida: el registro de pánicos de una corrida anterior
    // no puede hacer fallar la comprobación de «sin PÁNICO».
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    let temp = std::env::temp_dir().join(format!("ezagent-dx1d-{}-{stamp}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp);
    // El hook de pánico escribe el mensaje y la ubicación antes de abortar.
    ez_core::panics::install_hook_in(&temp.join("logs"));
    if let Err(error) = seed_config(&temp) {
        eprintln!("No pude escribir la configuración sintética: {error}");
        std::process::exit(1);
    }
    let content = if big {
        big_history_jsonl()
    } else if anim {
        anim_history_jsonl()
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
        if mode == "real" || anim {
            // El clic real del engranaje: barra → `WndProc` → `HUD: panel`.
            std::thread::sleep(Duration::from_secs(1));
            click_bar(&app, "gear");
            app.tick();
            eprintln!("clic del panel enviado");
            std::thread::sleep(Duration::from_secs(if anim { 1 } else { 3 }));
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
    if anim {
        // Solo los cuadros de la prueba: el diario está apagado por omisión.
        trace::enable(true);
        trace::clear();
    }
    if mode == "chat" || big {
        app.handle_hotkey("chat");
    } else if mode == "real" || anim {
        click_bar(&app, "chat");
        app.tick();
    } else {
        app.on_hud_action("chat");
        app.tick();
    }
    if anim {
        // El clic entra por la cola de la barra: se espera a que el panel abra.
        let deadline = Instant::now() + Duration::from_secs(3);
        while !app.hud().chat_is_open() && Instant::now() < deadline {
            app.tick();
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let opened = app.hud().chat_is_open();
    eprintln!("chat abierto: {opened}");

    if anim {
        let code = check_animation(&app, opened);
        let panics = panic_lines(&temp.join("logs"));
        if panics.is_empty() {
            say("registro temporal: sin PÁNICO");
        } else {
            say(&format!(
                "registro temporal: {} línea(s) de PÁNICO",
                panics.len()
            ));
            for line in &panics {
                say(line);
            }
        }
        let inside = check_monitor();
        app.shutdown();
        std::process::exit(if code == 0 && panics.is_empty() && inside {
            0
        } else {
            1
        });
    }

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
    eprintln!("RSS pico: {peak_mb} MB (décimas: {ticks})");
    if panics.is_empty() {
        eprintln!("registro temporal: sin PÁNICO");
    } else {
        eprintln!("registro temporal: {} línea(s) de PÁNICO", panics.len());
        for line in &panics {
            eprintln!("{line}");
        }
    }
    eprintln!("RESULT chat_open={alive} alive_secs={ALIVE_SECS}");
    let inside = check_monitor();
    app.shutdown();
    if !alive || !inside || !panics.is_empty() || peak_mb > 1024 {
        std::process::exit(1);
    }
}
