//! Bancada de pruebas: repositorio git real, reloj, lanzador y escritor falsos.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ez_arbiter::{
    Arbiter, Caller, Clock, Command, Delivery, FishKey, FishLauncher, InstructionWriter,
    Launched, LaunchRequest, Outcome, Rejection, Space, Verdict,
};
use ez_team::{InstructionContext, Role, Seat};

/// Reloj que no avanza solo.
pub struct FakeClock {
    seconds: Mutex<f64>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            seconds: Mutex::new(1000.0),
        }
    }

    pub fn advance(&self, seconds: f64) {
        let mut guard = self.seconds.lock().expect("bloquear el reloj");
        *guard += seconds;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> f64 {
        *self.seconds.lock().expect("leer el reloj")
    }
}

/// Lanzador que no abre terminales: guarda lo que recibe y las revocaciones.
pub struct FakeLauncher {
    requests: Mutex<Vec<LaunchRequest>>,
    sent: Mutex<Vec<(String, String)>>,
    alive: Mutex<HashSet<String>>,
    fail: Mutex<Option<String>>,
    delivery: Mutex<Delivery>,
    revoked: Mutex<Vec<String>>,
}

impl FakeLauncher {
    pub fn new() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            sent: Mutex::new(Vec::new()),
            alive: Mutex::new(HashSet::new()),
            fail: Mutex::new(None),
            delivery: Mutex::new(Delivery::Delivered),
            revoked: Mutex::new(Vec::new()),
        }
    }

    /// El siguiente `launch` devuelve `Err` con este detalle.
    pub fn fail_with(&self, detail: &str) {
        *self.fail.lock().expect("fijar el fallo") = Some(detail.to_string());
    }

    /// Fija el resultado de entrega del siguiente `launch`.
    pub fn deliver(&self, delivery: Delivery) {
        *self.delivery.lock().expect("fijar la entrega") = delivery;
    }

    pub fn revoked(&self) -> Vec<String> {
        self.revoked.lock().expect("leer revocaciones").clone()
    }

    pub fn kill(&self, terminal: &str) {
        self.alive
            .lock()
            .expect("bloquear terminales")
            .remove(terminal);
    }

    pub fn requests(&self) -> Vec<LaunchRequest> {
        self.requests.lock().expect("leer lanzamientos").clone()
    }

    pub fn sent(&self) -> Vec<(String, String)> {
        self.sent.lock().expect("leer mensajes").clone()
    }

    pub fn terminal(&self, fish: &str) -> String {
        let requests = self.requests();
        let index = requests
            .iter()
            .position(|request| request.fish == fish)
            .expect("pez lanzado");
        format!("t{}", index + 1)
    }
}

impl FishLauncher for FakeLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<Launched, String> {
        if let Some(detail) = self.fail.lock().expect("leer el fallo").clone() {
            return Err(detail);
        }
        let terminal = format!("t{}", self.requests.lock().expect("contar").len() + 1);
        let delivery = self.delivery.lock().expect("leer la entrega").clone();
        self.requests
            .lock()
            .expect("guardar lanzamiento")
            .push(request.clone());
        self.alive
            .lock()
            .expect("marcar viva")
            .insert(terminal.clone());
        Ok(Launched { terminal, delivery })
    }

    fn send(&self, terminal: &str, text: &str) -> Result<(), String> {
        self.sent
            .lock()
            .expect("guardar mensaje")
            .push((terminal.to_string(), text.to_string()));
        Ok(())
    }

    fn alive(&self, terminal: &str) -> bool {
        self.alive
            .lock()
            .expect("leer terminales")
            .contains(terminal)
    }

    fn revoke(&self, token: &str) {
        self.revoked
            .lock()
            .expect("guardar revocación")
            .push(token.to_string());
    }
}

/// Escritor de instrucciones falso.
pub struct FakeWriter;

impl InstructionWriter for FakeWriter {
    fn render(&self, context: &InstructionContext) -> Result<String, String> {
        Ok(format!(
            "# Instrucciones de {} ({}) para la ronda {}",
            context.fish,
            context.role.id(),
            context.round
        ))
    }

    fn first_line(&self, role: Role, instructions: &Path) -> String {
        format!(
            "Eres un pez dorado de EzAgent con el papel {}. Lee {} y síguelo al pie de la letra.",
            role.id(),
            instructions.display()
        )
    }
}

/// Bancada con una carpeta temporal única, borrada al terminar.
pub struct Fixture {
    pub root: PathBuf,
    pub folder: PathBuf,
    pub data_dir: PathBuf,
    pub arbiter: Arbiter,
    pub launcher: Arc<FakeLauncher>,
    pub clock: Arc<FakeClock>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    /// Crea un espacio con git inicializado y un primer commit.
    pub fn new(name: &str) -> Self {
        let unique = uuid::Uuid::new_v4().simple();
        let root = std::env::temp_dir().join(format!("ez-arbiter-{name}-{unique}"));
        fs::create_dir_all(&root).expect("crear la carpeta temporal");
        let folder = root.join("proyecto");
        let data_dir = root.join("datos");
        fs::create_dir_all(&folder).expect("crear el proyecto");
        fs::create_dir_all(&data_dir).expect("crear los datos");
        git(&folder, &["init", "-q"]);
        git(&folder, &["config", "user.name", "Pez de prueba"]);
        git(&folder, &["config", "user.email", "pez@example.test"]);
        git(&folder, &["commit", "--allow-empty", "-q", "-m", "inicial"]);
        let launcher = Arc::new(FakeLauncher::new());
        let clock = Arc::new(FakeClock::new());
        let arbiter = Arbiter::new(
            data_dir.clone(),
            launcher.clone(),
            Arc::new(FakeWriter),
            clock.clone(),
        )
        .expect("abrir el árbitro");
        arbiter.register_space(Self::space(&folder));
        Self {
            root,
            folder,
            data_dir,
            arbiter,
            launcher,
            clock,
        }
    }

    /// Igual que `new`, pero sin repositorio git (para R2).
    pub fn without_git(name: &str) -> Self {
        let fixture = Self::new(name);
        fs::remove_dir_all(fixture.folder.join(".git")).expect("quitar git");
        fixture
    }

    pub fn space(folder: &Path) -> Space {
        Space {
            id: "s1".to_string(),
            name: "Espacio de prueba".to_string(),
            folder: folder.to_path_buf(),
        }
    }

    pub fn elephant() -> Caller {
        Caller::Elephant {
            space: "s1".to_string(),
        }
    }

    /// Clave y ruta del informe pendiente de un pez, leídas del estado persistido.
    pub fn fish_info(&self, fish: &str) -> (FishKey, PathBuf) {
        let root = self.data_dir.join("tareas");
        for entry in fs::read_dir(&root).expect("leer tareas").flatten() {
            let path = entry.path().join("estado.json");
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let Some(records) = state["fish"].as_array() else {
                continue;
            };
            let Some(record) = records.iter().rev().find(|item| item["key"]["fish"] == fish)
            else {
                continue;
            };
            let key = FishKey {
                space: record["key"]["space"]
                    .as_str()
                    .expect("espacio")
                    .to_string(),
                task: record["key"]["task"].as_str().expect("tarea").to_string(),
                fish: record["key"]["fish"].as_str().expect("pez").to_string(),
                attempt: record["key"]["attempt"].as_u64().expect("intento") as u32,
            };
            let works = state["work"].as_array();
            let find_work = |state_filter: Option<&str>| -> Option<PathBuf> {
                works?.iter().rev().find_map(|item| {
                    let matches_key = item["key"]["fish"] == fish
                        && item["key"]["attempt"] == key.attempt;
                    let matches_state = state_filter.is_none()
                        || item["state"].as_str() == state_filter;
                    if matches_key && matches_state {
                        item["report_path"].as_str().map(PathBuf::from)
                    } else {
                        None
                    }
                })
            };
            let report_path = find_work(Some("pending"))
                .or_else(|| find_work(None))
                .expect("ruta del informe del pez");
            return (key, report_path);
        }
        panic!("pez no encontrado en el estado: {fish}");
    }

    pub fn fish_key(&self, fish: &str) -> FishKey {
        self.fish_info(fish).0
    }

    pub fn as_fish(&self, fish: &str) -> Caller {
        Caller::Fish(self.fish_key(fish))
    }

    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.folder.join(name);
        fs::write(&path, text).expect("escribir el archivo de prueba");
        path
    }

    pub fn criteria(&self, body: &str) -> PathBuf {
        self.write("criterios.toml", body)
    }

    pub fn default_criteria(&self) -> PathBuf {
        self.criteria(
            "[[criterio]]\nid = \"C1\"\ndescripcion = \"Git responde\"\n\
             comando = [\"git\", \"--version\"]\ncontiene = \"git version\"\n",
        )
    }

    pub fn brief(&self) -> PathBuf {
        self.write("encargo.md", "# Encargo\n\nHaz la tarea.\n")
    }

    /// Crea la tarea y devuelve su id.
    pub fn propose(&self) -> String {
        let outcome = self
            .arbiter
            .execute(
                &Self::elephant(),
                Command::TaskNew {
                    title: "Tarea de prueba".to_string(),
                    brief: self.brief(),
                    criteria: self.default_criteria(),
                },
            )
            .expect("proponer la tarea");
        outcome.data["task"]
            .as_str()
            .expect("id de tarea")
            .to_string()
    }

    pub fn tasks() -> Vec<Seat> {
        vec![
            Self::seat(Role::Planner, 1),
            Self::seat(Role::Developer, 1),
            Self::seat(Role::Qa, 1),
            Self::seat(Role::QaValidator, 1),
        ]
    }

    pub fn seat(role: Role, count: u32) -> Seat {
        Seat {
            role,
            agent: "falso".to_string(),
            model: "modelo".to_string(),
            effort: None,
            count,
        }
    }

    pub fn team(&self, seats: Vec<Seat>) {
        let card = "Equipo propuesto para «Tarea de prueba».".to_string();
        self.arbiter
            .execute(&Self::elephant(), Command::TeamPropose { seats, card })
            .expect("proponer el equipo");
    }

    /// Proponer, aprobar, abrir la ronda 1 y hacer que el planificador informe `pass`.
    pub fn setup(&self) -> String {
        let task = self.propose();
        self.arbiter.approve_task("s1").expect("aprobar tarea");
        self.team(Self::tasks());
        self.arbiter.approve_team("s1").expect("aprobar equipo");
        self.arbiter
            .execute(&Self::elephant(), Command::RoundStart)
            .expect("abrir la ronda");
        self.launch(Role::Planner);
        self.report("plan-r1", Verdict::Pass, None, None)
            .expect("informe del planificador");
        task
    }

    pub fn launch(&self, role: Role) -> String {
        let outcome = self
            .arbiter
            .execute(
                &Self::elephant(),
                Command::FishNew {
                    role,
                    brief: self.brief(),
                },
            )
            .expect("lanzar un pez");
        outcome.data["fish"]
            .as_str()
            .expect("id de pez")
            .to_string()
    }

    /// Informe con veredicto, mutaciones y archivo en la ruta exacta del trabajo.
    pub fn report(
        &self,
        fish: &str,
        verdict: Verdict,
        tried: Option<u32>,
        caught: Option<u32>,
    ) -> Result<Outcome, Rejection> {
        let (key, path) = self.fish_info(fish);
        if std::env::var_os("EZ_DEBUG_PATHS").is_some() {
            eprintln!("DEBUG fish={fish} key={key:?} path={}", path.display());
        }
        fs::write(&path, "Informe del pez.\n").expect("escribir el informe");
        self.arbiter.execute(
            &Caller::Fish(key),
            Command::Report {
                verdict,
                file: path,
                blocking: 0,
                major: 0,
                minor: 0,
                mutations_tried: tried,
                mutations_caught: caught,
            },
        )
    }

    pub fn close_round(&self) -> Result<Outcome, Rejection> {
        self.arbiter.execute(&Self::elephant(), Command::RoundClose)
    }

    pub fn state_json(&self, task: &str) -> serde_json::Value {
        let path = self.data_dir.join("tareas").join(task).join("estado.json");
        let text = fs::read_to_string(path).expect("leer estado.json");
        serde_json::from_str(&text).expect("estado.json válido")
    }

    pub fn bitacora(&self, task: &str) -> Vec<serde_json::Value> {
        let path = self
            .data_dir
            .join("tareas")
            .join(task)
            .join("bitacora.jsonl");
        let text = fs::read_to_string(path).unwrap_or_default();
        text.lines()
            .map(|line| serde_json::from_str(line).expect("línea jsonl válida"))
            .collect()
    }

    pub fn snapshot(&self) -> serde_json::Value {
        self.arbiter
            .snapshot("s1")
            .expect("instantánea del espacio")
    }
}

/// Ejecuta git sin shell y sin ventana de consola.
pub fn git(folder: &Path, args: &[&str]) {
    let mut command = std::process::Command::new("git");
    command.arg("-C").arg(folder).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let status = command.status().expect("ejecutar git");
    assert!(status.success(), "git {args:?} falló");
}
