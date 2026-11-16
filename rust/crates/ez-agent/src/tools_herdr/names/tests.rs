//! Pruebas del resolvedor de las herramientas: con el texto real del log
//! («TES» → «TEST»), con un puerto falso y con el gate respondiendo sí, no y
//! sin respuesta.

use std::sync::{Arc, Mutex};

use ez_herdr::herdr::{NameCandidate, NameKind, RunAgent};
use serde_json::{json, Map, Value};

use super::*;
use crate::registry::ToolResult;
use crate::tools_herdr::args::{shared_state, HerdrCallbacks, NameConfirm};
use crate::tools_herdr::delegate::handle_delegate;
use crate::tools_herdr::port::{HerdrPort, PortError};

fn candidates(pairs: &[(&str, &str)]) -> Vec<NameCandidate> {
    pairs
        .iter()
        .map(|(value, label)| NameCandidate::new(*value, *label))
        .collect()
}

#[derive(Default)]
struct Fake {
    spaces: Vec<NameCandidate>,
    terminals: Vec<NameCandidate>,
    agents: Vec<NameCandidate>,
    seen: Mutex<Vec<String>>,
}

impl Fake {
    fn new() -> Self {
        Self {
            spaces: candidates(&[("TEST", "TEST"), ("EzAgent", "EzAgent")]),
            terminals: candidates(&[("t3", "TEST"), ("t1", "planner")]),
            agents: candidates(&[("opencode", "OpenCode"), ("claude", "Claude Code")]),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn seen(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn record(&self, line: String) -> Map<String, Value> {
        self.seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(line);
        json!({"ok": true}).as_object().cloned().unwrap_or_default()
    }
}

impl HerdrPort for Fake {
    fn resolve_agent(&self, name: &str) -> Option<String> {
        self.agents
            .iter()
            .find(|candidate| candidate.value == name)
            .map(|candidate| candidate.value.clone())
    }

    fn label_for(&self, _address: &str) -> String {
        String::new()
    }

    fn name_candidates(&self, kind: NameKind, _fresh: bool) -> Vec<NameCandidate> {
        match kind {
            NameKind::Space => self.spaces.clone(),
            NameKind::Terminal => self.terminals.clone(),
            NameKind::Agent => self.agents.clone(),
        }
    }

    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, PortError> {
        Ok(Vec::new())
    }

    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        Ok(self.record(format!("get {address}")))
    }

    fn read_agent(&self, address: &str, _lines: Option<i64>) -> Result<String, PortError> {
        self.record(format!("read {address}"));
        Ok("pantalla".to_string())
    }

    fn read_full(&self, address: &str) -> Result<String, PortError> {
        self.record(format!("read_full {address}"));
        Ok("salida del agente".to_string())
    }

    fn send_agent(
        &self,
        address: &str,
        _text: &str,
        _confirmed: bool,
    ) -> Result<Map<String, Value>, PortError> {
        Ok(self.record(format!("send {address}")))
    }

    fn run_agent(&self, params: RunAgent<'_>) -> Result<Map<String, Value>, PortError> {
        self.record(format!("run {} {:?}", params.agent, params.space));
        let mut result = Map::new();
        result.insert("ok".to_string(), Value::Bool(true));
        result.insert("launching".to_string(), Value::Bool(true));
        result.insert("address".to_string(), Value::String("t9".to_string()));
        Ok(result)
    }

    fn open_terminal(
        &self,
        name: &str,
        _cwd: Option<&str>,
        _shell: &str,
        space: Option<&str>,
    ) -> Result<Map<String, Value>, PortError> {
        self.record(format!("open_terminal {name} {space:?}"));
        let mut result = Map::new();
        result.insert("ok".to_string(), Value::Bool(true));
        result.insert("terminal".to_string(), Value::String("t5".to_string()));
        Ok(result)
    }

    fn agents_catalog(&self) -> Vec<Map<String, Value>> {
        Vec::new()
    }

    fn configure_agent(
        &self,
        address: &str,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> Map<String, Value> {
        self.record(format!("configure {address}"))
    }

    fn keys_agent(&self, address: &str, _keys: &[String]) -> Result<Map<String, Value>, PortError> {
        Ok(self.record(format!("keys {address}")))
    }

    fn focus_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        Ok(self.record(format!("focus {address}")))
    }

    fn kill_agent(&self, address: &str, _confirmed: bool) -> Result<Map<String, Value>, PortError> {
        Ok(self.record(format!("kill {address}")))
    }

    fn save_context(&self, _text: &str) -> Option<String> {
        None
    }
}

type Asked = Arc<Mutex<Vec<String>>>;

fn gate(answer: bool) -> (NameConfirm, Asked) {
    let asked: Asked = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&asked);
    let confirm: NameConfirm = Arc::new(move |question: &str| {
        recorded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(question.to_string());
        answer
    });
    (confirm, asked)
}

fn call(fake: &Fake, confirm: Option<NameConfirm>, args: Value) -> ToolResult {
    let callbacks = HerdrCallbacks {
        confirm_name: confirm,
        ..HerdrCallbacks::default()
    };
    handle_delegate(
        fake,
        &callbacks,
        &shared_state(),
        args.as_object().expect("objeto"),
    )
}

#[test]
fn una_coincidencia_exacta_no_pregunta_ni_deja_nota() {
    let fake = Fake::new();
    let (confirm, asked) = gate(true);
    let result = call(
        &fake,
        Some(confirm),
        json!({"action": "get", "address": "planner"}),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(fake.seen(), vec!["get t1".to_string()]);
    assert!(asked.lock().expect("preguntas").is_empty());
    assert!(result.data.get(NOTE_FIELD).is_none());
}

#[test]
fn un_nombre_muy_probable_se_usa_y_se_dice() {
    let fake = Fake::new();
    let (confirm, asked) = gate(false);
    let result = call(
        &fake,
        Some(confirm),
        json!({"action": "get", "address": "planer"}),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(fake.seen(), vec!["get t1".to_string()]);
    assert!(asked.lock().expect("preguntas").is_empty());
    assert_eq!(result.data[NOTE_FIELD], json!("usé la terminal «planner»"));
}

#[test]
fn una_duda_con_si_sigue_con_el_nombre_resuelto() {
    let fake = Fake::new();
    let (confirm, asked) = gate(true);
    let result = call(
        &fake,
        Some(confirm),
        json!({"action": "get", "address": "TES"}),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(fake.seen(), vec!["get t3".to_string()]);
    assert_eq!(
        asked.lock().expect("preguntas").as_slice(),
        ["¿Te refieres a la terminal «TEST»?".to_string()]
    );
    assert_eq!(result.data[NOTE_FIELD], json!("usé la terminal «TEST»"));
}

#[test]
fn una_duda_con_no_lista_los_nombres() {
    let fake = Fake::new();
    let (confirm, _) = gate(false);
    let result = call(
        &fake,
        Some(confirm),
        json!({"action": "get", "address": "TES"}),
    );
    assert!(!result.ok);
    assert_eq!(
        result.error.as_deref(),
        Some("No encontré la terminal «TES». Terminales abiertas: «planner», «TEST».")
    );
    assert!(fake.seen().is_empty());
}

#[test]
fn una_duda_sin_respuesta_tambien_lista_los_nombres() {
    let fake = Fake::new();
    let result = call(
        &fake,
        None,
        json!({"action": "send", "address": "TES", "task": "hola"}),
    );
    assert!(!result.ok);
    assert_eq!(
        result.error.as_deref(),
        Some("No encontré la terminal «TES». Terminales abiertas: «planner», «TEST».")
    );
    assert!(fake.seen().is_empty());
}

#[test]
fn un_nombre_sin_parecido_lista_los_nombres() {
    let fake = Fake::new();
    let result = call(&fake, None, json!({"action": "kill", "address": "zzz"}));
    assert!(!result.ok);
    assert_eq!(
        result.error.as_deref(),
        Some("No encontré la terminal «zzz». Terminales abiertas: «planner», «TEST».")
    );
}

#[test]
fn el_espacio_dudoso_se_confirma_antes_de_abrirlo() {
    let fake = Fake::new();
    let (confirm, asked) = gate(true);
    let result = call(
        &fake,
        Some(confirm),
        json!({"action": "open_terminal", "name": "planner", "space": "TES"}),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(
        fake.seen(),
        vec!["open_terminal planner Some(\"TEST\")".to_string()]
    );
    assert_eq!(
        asked.lock().expect("preguntas").as_slice(),
        ["¿Te refieres al espacio «TEST»?".to_string()]
    );
    assert_eq!(result.data[NOTE_FIELD], json!("usé el espacio «TEST»"));
}

#[test]
fn el_agente_mal_transcrito_llega_al_lanzamiento_resuelto() {
    let fake = Fake::new();
    let (confirm, asked) = gate(false);
    let result = call(
        &fake,
        Some(confirm),
        json!({"action": "run", "agent": "OpenCobe"}),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(fake.seen(), vec!["run opencode None".to_string()]);
    assert!(asked.lock().expect("preguntas").is_empty());
    assert_eq!(result.data[NOTE_FIELD], json!("usé el agente «OpenCode»"));
}

#[test]
fn la_direccion_de_origen_tambien_se_resuelve() {
    let fake = Fake::new();
    let (confirm, _) = gate(true);
    let result = call(
        &fake,
        Some(confirm),
        json!({"action": "run", "agent": "claude", "input_from": "planer"}),
    );
    assert!(result.ok, "{:?}", result.error);
    let seen = fake.seen();
    assert_eq!(seen.first().map(String::as_str), Some("get t1"));
    assert!(seen.contains(&"read_full t1".to_string()));
}

#[test]
fn sin_candidatos_se_usa_el_texto_tal_cual() {
    struct Empty;
    impl HerdrPort for Empty {
        fn resolve_agent(&self, name: &str) -> Option<String> {
            Some(name.to_string())
        }
        fn label_for(&self, _address: &str) -> String {
            String::new()
        }
        fn list_agents(&self) -> Result<Vec<Map<String, Value>>, PortError> {
            Ok(Vec::new())
        }
        fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, PortError> {
            Ok(Map::new())
        }
        fn read_agent(&self, _address: &str, _lines: Option<i64>) -> Result<String, PortError> {
            Ok(String::new())
        }
        fn read_full(&self, _address: &str) -> Result<String, PortError> {
            Ok(String::new())
        }
        fn send_agent(
            &self,
            _address: &str,
            _text: &str,
            _confirmed: bool,
        ) -> Result<Map<String, Value>, PortError> {
            Ok(Map::new())
        }
        fn run_agent(&self, _params: RunAgent<'_>) -> Result<Map<String, Value>, PortError> {
            Ok(Map::new())
        }
        fn open_terminal(
            &self,
            _name: &str,
            _cwd: Option<&str>,
            _shell: &str,
            _space: Option<&str>,
        ) -> Result<Map<String, Value>, PortError> {
            Ok(Map::new())
        }
        fn agents_catalog(&self) -> Vec<Map<String, Value>> {
            Vec::new()
        }
        fn configure_agent(
            &self,
            _address: &str,
            _model: Option<&str>,
            _effort: Option<&str>,
        ) -> Map<String, Value> {
            Map::new()
        }
        fn keys_agent(
            &self,
            _address: &str,
            _keys: &[String],
        ) -> Result<Map<String, Value>, PortError> {
            Ok(Map::new())
        }
        fn focus_agent(&self, _address: &str) -> Result<Map<String, Value>, PortError> {
            Ok(Map::new())
        }
        fn kill_agent(
            &self,
            _address: &str,
            _confirmed: bool,
        ) -> Result<Map<String, Value>, PortError> {
            Ok(Map::new())
        }
        fn save_context(&self, _text: &str) -> Option<String> {
            None
        }
    }
    let names = Names::new(&Empty, None);
    assert_eq!(names.pick(NameKind::Space, " TES "), Ok("TES".to_string()));
}
