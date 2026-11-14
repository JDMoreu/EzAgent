//! Bucle de la interfaz (`root.mainloop` + `root.after`, F6-01).
//!
//! El Python usa Tk en el hilo principal: una cola de eventos que despierta el
//! `mainloop` y `root.after(ms, callback)` para lo diferido. Aquí no hay Tk:
//! una ventana **solo de mensajes** vive en el hilo que llama a [`UiLoop::run`]
//! y `PostMessageW(WM_APP)` despierta su cola. Los callbacks se ejecutan fuera
//! del `WndProc`, así que un pánico no cruza la frontera FFI.
//!
//! // DECISION: `post` y `schedule` nunca bloquean al llamador; el trabajo se
//! // ejecuta en el hilo del bucle, igual que `root.after(0, …)`.
//!
//! El código nativo solo aparece en este submódulo de `win/` (AGENTS.rust.md §5).

use std::collections::{BTreeMap, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU16, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MsgWaitForMultipleObjects,
    PeekMessageW, PostMessageW, RegisterClassW, TranslateMessage, HWND_MESSAGE, MSG, PM_REMOVE,
    QS_ALLINPUT, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_QUIT, WNDCLASSW,
};

/// Mensaje que despierta la cola (`PostMessage(WM_APP)`).
const WM_WAKE: u32 = WM_APP + 1;
const WINDOW_CLASS: windows::core::PCWSTR = w!("EzAgentUiLoop");
static CLASS_ATOM: AtomicU16 = AtomicU16::new(0);

/// Identificador de un temporizador de [`UiLoop::schedule`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimerId(pub u64);

type Task = Box<dyn FnOnce() + Send + 'static>;

struct Timer {
    deadline: Instant,
    task: Task,
}

#[derive(Default)]
struct Scheduler {
    queue: Mutex<VecDeque<Task>>,
    timers: Mutex<BTreeMap<u64, Timer>>,
    next: AtomicU64,
    quit: AtomicBool,
}

impl Scheduler {
    fn post(&self, task: Task) {
        self.queue.lock().push_back(task);
    }

    fn schedule(&self, delay: Duration, task: Task) -> TimerId {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let deadline = Instant::now()
            .checked_add(delay)
            .unwrap_or_else(Instant::now);
        self.timers.lock().insert(id, Timer { deadline, task });
        TimerId(id)
    }

    fn cancel(&self, id: TimerId) -> bool {
        self.timers.lock().remove(&id.0).is_some()
    }

    fn quit(&self) {
        self.quit.store(true, Ordering::SeqCst);
    }

    fn quitting(&self) -> bool {
        self.quit.load(Ordering::SeqCst)
    }

    /// Ejecuta lo que ya toca: temporizadores vencidos (en orden de vencimiento)
    /// y después la cola, como una vuelta de `mainloop`.
    fn run_ready(&self) {
        let now = Instant::now();
        let due: Vec<Task> = {
            let mut timers = self.timers.lock();
            let due_ids: Vec<u64> = timers
                .iter()
                .filter(|(_, timer)| timer.deadline <= now)
                .map(|(id, _)| *id)
                .collect();
            due_ids
                .into_iter()
                .filter_map(|id| timers.remove(&id).map(|timer| timer.task))
                .collect()
        };
        for task in due {
            run_task(task);
        }
        loop {
            let task = self.queue.lock().pop_front();
            match task {
                Some(task) => run_task(task),
                None => break,
            }
        }
    }

    /// Milisegundos hasta el próximo temporizador (`u32::MAX` = sin plazo).
    fn next_timeout_ms(&self) -> u32 {
        let next = self
            .timers
            .lock()
            .values()
            .map(|timer| timer.deadline)
            .min();
        let Some(deadline) = next else {
            return u32::MAX;
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        remaining.as_millis().min(f64::from(u32::MAX) as u128) as u32
    }

    fn pending(&self) -> usize {
        self.queue.lock().len() + self.timers.lock().len()
    }
}

fn run_task(task: Task) {
    // Un pánico del callback no debe tumbar el bucle de la interfaz.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
}

struct Inner {
    scheduler: Scheduler,
    hwnd: AtomicIsize,
    running: AtomicBool,
}

/// Cola de la interfaz: mensajes y temporizadores del hilo principal.
#[derive(Clone)]
pub struct UiLoop {
    inner: Arc<Inner>,
}

impl Default for UiLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl UiLoop {
    /// Crea la cola sin abrir ninguna ventana (eso lo hace [`UiLoop::run`]).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                scheduler: Scheduler::default(),
                hwnd: AtomicIsize::new(0),
                running: AtomicBool::new(false),
            }),
        }
    }

    /// Encola un callback para el hilo del bucle (`root.after(0, …)`).
    pub fn post<F>(&self, task: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.inner.scheduler.post(Box::new(task));
        self.wake();
    }

    /// Programa un callback dentro de `delay` (`root.after(ms, …)`).
    pub fn schedule<F>(&self, delay: Duration, task: F) -> TimerId
    where
        F: FnOnce() + Send + 'static,
    {
        let id = self.inner.scheduler.schedule(delay, Box::new(task));
        self.wake();
        id
    }

    /// Cancela un temporizador pendiente; `true` si aún no se había ejecutado.
    pub fn cancel(&self, id: TimerId) -> bool {
        self.inner.scheduler.cancel(id)
    }

    /// Despierta la cola (`PostMessage(WM_APP)`); no espera a que se atienda.
    pub fn wake(&self) {
        let hwnd = self.inner.hwnd.load(Ordering::Acquire);
        if hwnd == 0 {
            return;
        }
        // SAFETY: `hwnd` es la ventana solo de mensajes creada por `run` en este
        // proceso; el mensaje no lleva punteros y puede descartarse.
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut c_void)),
                WM_WAKE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    /// Ejecuta el bucle en el hilo actual hasta [`UiLoop::quit`].
    pub fn run(&self) -> Result<(), String> {
        if self.inner.scheduler.quitting() {
            return Ok(());
        }
        if self.inner.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let hwnd = match create_message_window() {
            Ok(hwnd) => hwnd,
            Err(error) => {
                self.inner.running.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };
        self.inner.hwnd.store(hwnd.0 as isize, Ordering::Release);
        self.message_loop();
        self.inner.hwnd.store(0, Ordering::Release);
        // SAFETY: la ventana es de este hilo y ya no se usa.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        self.inner.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Pide terminar el bucle; el trabajo pendiente se descarta.
    pub fn quit(&self) {
        self.inner.scheduler.quit();
        self.wake();
    }

    /// `true` mientras el bucle se está ejecutando.
    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::Acquire)
    }

    /// Callbacks pendientes (mensajes y temporizadores).
    pub fn pending(&self) -> usize {
        self.inner.scheduler.pending()
    }

    fn message_loop(&self) {
        let mut message = MSG::default();
        while !self.inner.scheduler.quitting() {
            self.inner.scheduler.run_ready();
            let timeout = self.inner.scheduler.next_timeout_ms();
            // SAFETY: espera sobre la cola de mensajes del hilo actual; sin
            // handles propios y con un plazo válido (`u32::MAX` = infinito).
            unsafe {
                MsgWaitForMultipleObjects(None, false, timeout, QS_ALLINPUT);
            }
            // SAFETY: los mensajes los produce PeekMessageW para este hilo y el
            // `MSG` es local.
            unsafe {
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    if message.message == WM_QUIT {
                        self.inner.scheduler.quit();
                        break;
                    }
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
    }
}

fn create_message_window() -> Result<HWND, String> {
    // SAFETY: el nombre del módulo es estático y el proceso sigue vivo.
    let instance = unsafe { GetModuleHandleW(windows::core::PCWSTR::null()) }
        .map_err(|error| format!("No pude obtener el módulo de la app: {error}"))?;
    let instance = HINSTANCE(instance.0);
    if CLASS_ATOM.load(Ordering::Acquire) == 0 {
        let class = WNDCLASSW {
            hInstance: instance,
            lpfnWndProc: Some(wndproc),
            lpszClassName: WINDOW_CLASS,
            ..Default::default()
        };
        // SAFETY: `class` y el nombre estático viven durante la llamada.
        let atom = unsafe { RegisterClassW(&class) };
        if atom == 0 {
            return Err("No pude registrar la clase del bucle de la interfaz.".to_string());
        }
        CLASS_ATOM.store(atom, Ordering::Release);
    }
    // SAFETY: la clase está registrada en este hilo; `HWND_MESSAGE` crea una
    // ventana solo de mensajes, sin UI visible.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WINDOW_CLASS,
            w!("EzAgentUiLoop"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )
        .map_err(|error| format!("No pude crear la ventana del bucle: {error}"))
    }
}

/// Procesal por omisión: los mensajes solo despiertan la cola.
extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: los cuatro parámetros los entrega Windows y se reenvían sin tocar.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn posts_run_fifo() {
        let scheduler = Scheduler::default();
        let (tx, rx) = mpsc::channel();
        for index in 1..=3 {
            let tx = tx.clone();
            scheduler.post(Box::new(move || {
                let _ = tx.send(index);
            }));
        }
        scheduler.run_ready();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn timers_fire_when_due_and_cancel_stops_them() {
        let scheduler = Scheduler::default();
        let (tx, rx) = mpsc::channel();
        let fired = {
            let tx = tx.clone();
            scheduler.schedule(
                Duration::ZERO,
                Box::new(move || {
                    let _ = tx.send("due");
                }),
            )
        };
        let cancelled = scheduler.schedule(
            Duration::from_secs(60),
            Box::new(move || {
                let _ = tx.send("late");
            }),
        );
        assert!(scheduler.cancel(cancelled));
        assert!(!scheduler.cancel(cancelled));
        scheduler.run_ready();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec!["due"]);
        assert_eq!(scheduler.pending(), 0);
        assert!(fired.0 > 0);
    }

    #[test]
    fn next_timeout_expresses_the_nearest_deadline() {
        let scheduler = Scheduler::default();
        assert_eq!(scheduler.next_timeout_ms(), u32::MAX);
        let due = scheduler.schedule(Duration::ZERO, Box::new(|| {}));
        assert_eq!(scheduler.next_timeout_ms(), 0);
        scheduler.run_ready();
        assert!(scheduler.cancel(due) || scheduler.pending() == 0);
        let id = scheduler.schedule(Duration::from_secs(5), Box::new(|| {}));
        let timeout = scheduler.next_timeout_ms();
        assert!(timeout > 4_000 && timeout <= 5_000);
        assert!(scheduler.cancel(id));
    }

    #[test]
    fn quit_is_sticky_and_pending_work_is_dropped() {
        let scheduler = Scheduler::default();
        let (tx, rx) = mpsc::channel();
        scheduler.quit();
        scheduler.post(Box::new(move || {
            let _ = tx.send(());
        }));
        assert!(scheduler.quitting());
        assert_eq!(scheduler.pending(), 1);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn loop_can_queue_before_running() {
        let ui = UiLoop::new();
        ui.post(|| {});
        let timer = ui.schedule(Duration::from_millis(10), || {});
        assert_eq!(ui.pending(), 2);
        assert!(ui.cancel(timer));
        assert_eq!(ui.pending(), 1);
        assert!(!ui.is_running());
        ui.quit();
    }

    #[test]
    fn a_panicking_callback_does_not_stop_the_queue() {
        let scheduler = Scheduler::default();
        let (tx, rx) = mpsc::channel();
        scheduler.post(Box::new(|| panic!("callback roto")));
        scheduler.post(Box::new(move || {
            let _ = tx.send("vivo");
        }));
        scheduler.run_ready();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec!["vivo"]);
    }
}
