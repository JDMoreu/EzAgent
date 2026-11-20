//! DX-1d: un pánico dentro de un callback de Windows no mata la app.
//!
//! La app se cayó al abrir el chat con `0xc0000409` (el aborto rápido de
//! Rust). Ese código es el que produce un pánico que intenta desenrollarse a
//! través de un `extern "system"`: Windows no puede propagarlo y Rust aborta.
//!
//! Este ejemplo pone un `WindowHooks` que entra en pánico en `pointer_down` y
//! entrega un clic real a la ventana. **Antes** del blindaje el proceso muere
//! con `0xc0000409`; **después** el pánico se atrapa, se escribe en el registro
//! de la app y el proceso sigue vivo.
//!
//! ```text
//! cargo run --release -p ez-hud --example panic_boundary
//! ```
//!
//! DX-7b: la ventana se abre **solo** en el monitor secundario. Antes de
//! crearla se comprueba que la secundaria exista; si no, la prueba falla sin
//! abrir ventanas. No hay ruta de prueba hacia la pantalla principal. Al final
//! imprime el rectángulo de la ventana y falla si quedó fuera del monitor
//! pedido.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_hud::win::layered::{LayeredWindow, RgbaFrame, WindowHooks, WindowOptions};
use ez_hud::win::monitor::{
    monitor_rect, rect_inside, require_secondary_monitor, work_area, TEST_MONITOR_ENV,
};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, PostMessageW, WM_LBUTTONDOWN};

/// Escribe una línea del informe (el ejemplo no usa `eprintln!`).
fn say(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{text}");
}

/// El clic llegó al callback (antes del pánico, para poder esperarlo).
static HIT: AtomicBool = AtomicBool::new(false);

/// Un callback que entra en pánico, como el que tumbó la app.
struct PanicHook;

impl WindowHooks for PanicHook {
    fn on_pointer_down(&mut self, _x: i32, _y: i32) -> bool {
        HIT.store(true, Ordering::SeqCst);
        panic!("DX-1d: pánico de prueba dentro de un callback de Windows")
    }
}

fn main() {
    // DX-7b: sin secundaria verificada no se crea ninguna ventana.
    std::env::set_var(TEST_MONITOR_ENV, "secondary");
    match require_secondary_monitor() {
        Ok(rect) => say(&format!("monitor verificado: secundario · rect {rect:?}")),
        Err(reason) => {
            say(&format!("no se abre la prueba: {reason}"));
            std::process::exit(2);
        }
    }
    let work = work_area();
    let dir = std::env::temp_dir().join(format!("ezagent-dx1d-panic-{}", std::process::id()));
    // El hook escribe el mensaje y la ubicación de forma síncrona: el registro
    // del `tracing` es asíncrono y se pierde al abortar.
    ez_core::panics::install_hook_in(&dir);

    let mut options = WindowOptions::new(64, 64);
    options.x = work.0;
    options.y = work.1;
    options.title = String::from("dx1d-panic");
    options.interval = 1.0 / 60.0;
    let window = Arc::new(LayeredWindow::new(options));
    window.set_hooks(PanicHook);
    if !window.start() {
        eprintln!("sin escritorio: no se puede probar la frontera");
        std::process::exit(2);
    }
    // Un fotograma opaco: `WM_LBUTTONDOWN` solo llega al callback si el píxel
    // tiene alfa (igual que el panel del chat).
    let mut pixels = vec![0u8; 64 * 64 * 4];
    for pixel in pixels.chunks_mut(4) {
        pixel.copy_from_slice(&[32, 32, 32, 255]);
    }
    let Some(frame) = RgbaFrame::new(64, 64, pixels) else {
        eprintln!("fotograma inválido");
        std::process::exit(2);
    };
    window.present(frame);
    std::thread::sleep(Duration::from_millis(300));

    let hwnd = window.hwnd();
    if hwnd == 0 {
        eprintln!("la ventana no nació");
        std::process::exit(2);
    }
    let point = LPARAM(((10_isize) << 16) | 10);
    let window_handle = windows::Win32::Foundation::HWND(hwnd as *mut _);
    // Se reintenta: el primer fotograma puede no estar presentado todavía y
    // `WM_LBUTTONDOWN` solo llega al callback si el píxel tiene alfa.
    let deadline = Instant::now() + Duration::from_secs(3);
    while !HIT.load(Ordering::SeqCst) && Instant::now() < deadline {
        // SAFETY: la ventana es propia y el mensaje lo atiende su WndProc.
        unsafe {
            let _ = PostMessageW(Some(window_handle), WM_LBUTTONDOWN, WPARAM(1), point);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    // El hook escribe la línea antes de desenrollar: si el proceso sigue vivo
    // y la línea está, el pánico no cruzó la frontera FFI.
    std::thread::sleep(Duration::from_millis(300));
    // DX-7: la ventana tiene que haber quedado dentro del monitor pedido.
    let mut rect = windows::Win32::Foundation::RECT::default();
    // SAFETY: la ventana es propia y `rect` es local.
    let inside = unsafe { GetWindowRect(window_handle, &mut rect) }.is_ok()
        && rect_inside(
            (rect.left, rect.top, rect.right, rect.bottom),
            monitor_rect(None, None),
        );
    say(&format!(
        "ventana dx1d-panic: rect ({}, {}, {}, {}) dentro={inside}",
        rect.left, rect.top, rect.right, rect.bottom
    ));
    let mut found = false;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(text) = std::fs::read_to_string(&path) {
                if text.contains("PÁNICO") {
                    found = true;
                    print!("{text}");
                }
            }
        }
    }
    window.stop(true);
    if !found {
        eprintln!("el clic no llegó a un callback que entre en pánico");
        std::process::exit(1);
    }
    if !inside {
        say("la ventana se salió del monitor pedido");
        std::process::exit(1);
    }
    println!("VIVO: el pánico quedó atrapado en el callback");
}
