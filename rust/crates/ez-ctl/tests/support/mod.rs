//! Ayudas compartidas por las pruebas de `ez-ctl`.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ez_ctl::{Caller, CtlServer, Handler, Reply, Request, Tokens};

pub const TOKEN: &str = "token-elefante";
pub const FISH_TOKEN: &str = "token-pez";
pub const SPACE: &str = "espacio";

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Carpeta temporal propia con nombre único, borrada al terminar.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new() -> TempDir {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos());
        let path = std::env::temp_dir().join(format!("ez-ctl-{}-{id}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("carpeta temporal");
        TempDir { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Crea un archivo con contenido de relleno y devuelve su ruta absoluta.
    pub fn file(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, "contenido").expect("archivo temporal");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// `Handler` que registra lo recibido y puede retener un comando a propósito.
#[derive(Default)]
pub struct FakeHandler {
    calls: Mutex<Vec<(Caller, Request)>>,
    delays: Mutex<HashMap<String, Duration>>,
}

impl FakeHandler {
    pub fn new() -> Arc<FakeHandler> {
        Arc::new(FakeHandler::default())
    }

    pub fn delay(&self, command: &str, delay: Duration) {
        self.delays
            .lock()
            .expect("demoras")
            .insert(command.to_string(), delay);
    }

    pub fn calls(&self) -> Vec<(Caller, Request)> {
        self.calls.lock().expect("registro").clone()
    }
}

impl Handler for FakeHandler {
    fn handle(&self, caller: &Caller, request: &Request) -> Reply {
        self.calls
            .lock()
            .expect("registro")
            .push((caller.clone(), request.clone()));
        let delay = self
            .delays
            .lock()
            .expect("demoras")
            .get(&request.command)
            .copied();
        if let Some(delay) = delay {
            std::thread::sleep(delay);
        }
        Reply {
            ok: true,
            message: format!("hecho: {}", request.command),
            data: serde_json::json!({ "eco": request.command }),
        }
    }
}

/// Servidor real en un puerto libre con su [`FakeHandler`].
pub struct Harness {
    pub dir: TempDir,
    pub handler: Arc<FakeHandler>,
    pub url: String,
    server: Option<CtlServer>,
}

impl Harness {
    pub fn new() -> Harness {
        let dir = TempDir::new();
        let handler = FakeHandler::new();
        let tokens = Arc::new(Tokens::new());
        tokens.register(
            TOKEN,
            Caller::Elephant {
                space: SPACE.to_string(),
            },
        );
        tokens.register(
            FISH_TOKEN,
            Caller::Fish {
                space: SPACE.to_string(),
                fish: "dev-r1-1".to_string(),
            },
        );
        let server = CtlServer::start(handler.clone(), tokens).expect("servidor");
        let url = server.url();
        Harness {
            dir,
            handler,
            url,
            server: Some(server),
        }
    }

    pub fn run(&self, argv: &[&str]) -> (i32, String, String) {
        self.run_as(TOKEN, argv)
    }

    pub fn run_as(&self, token: &str, argv: &[&str]) -> (i32, String, String) {
        run_at(&self.url, token, self.dir.path(), argv)
    }

    pub fn stop(&mut self) {
        if let Some(server) = self.server.take() {
            server.stop();
        }
    }
}

/// Antepone `ez` a los argumentos, como haría el proceso real.
pub fn program(argv: &[&str]) -> Vec<String> {
    std::iter::once("ez".to_string())
        .chain(argv.iter().map(|arg| arg.to_string()))
        .collect()
}

/// Ejecuta `main_with` con el entorno, la carpeta y los buffers dados.
pub fn run(
    argv: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = ez_ctl::main_with(argv, env, cwd, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

/// Ejecuta `ez` contra `url` con `token`, sin depender de variables del proceso.
pub fn run_at(url: &str, token: &str, cwd: &Path, argv: &[&str]) -> (i32, String, String) {
    let url = url.to_string();
    let token = token.to_string();
    let env = move |name: &str| match name {
        "EZ_URL" => Some(url.clone()),
        "EZ_TOKEN" => Some(token.clone()),
        _ => None,
    };
    run(&program(argv), &env, cwd)
}

/// Ejecuta `ez` sin `EZ_URL` ni `EZ_TOKEN`.
pub fn run_without_env(argv: &[&str], cwd: &Path) -> (i32, String, String) {
    run(&program(argv), &|_| None, cwd)
}
