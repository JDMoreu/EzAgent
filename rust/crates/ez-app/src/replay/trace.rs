//! Traza normalizada del arnés (`Trace` de `parity/replay/run_py.py`).
//!
//! Una línea JSON por evento, agrupada por hilo y ordenada dentro de cada hilo:
//! el orden entre hilos no es determinista en ninguna de las dos versiones.
//! `seq` es el ordinal del evento dentro de su etiqueta (`<nombre>#<n>`).
//! Nada volátil: el directorio temporal se sustituye por `<tmp>` y los puertos
//! locales por `127.0.0.1:<port>`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread::ThreadId;

use ez_core::pyjson::{dumps_with, DumpOptions};
use parking_lot::Mutex;
use serde_json::{Map, Value};

/// Orden de agrupación del archivo (igual que `THREAD_ORDER` del Python).
pub const THREAD_ORDER: [&str; 11] = [
    "main",
    "ezagent-call",
    "ezagent-stt",
    "ezagent-answer",
    "ezagent-speech-synth",
    "ezagent-speech",
    "ezagent-tts",
    "ezagent-busy",
    "ezagent-wake",
    "services",
    "audit",
];

#[derive(Default)]
struct State {
    events: Vec<Value>,
    keys: HashMap<ThreadId, String>,
    seq: HashMap<String, u64>,
    ordinals: HashMap<String, u64>,
}

/// Eventos por hilo con secuencia propia: el archivo queda determinista.
pub struct Trace {
    home: String,
    /// Id del enjambre que se sustituye por `<swarm>` (el `_scrub` del Python).
    scrub: parking_lot::RwLock<String>,
    state: Mutex<State>,
}

impl Trace {
    /// Traza vacía que normaliza rutas bajo `home`.
    pub fn new(home: &Path) -> Arc<Self> {
        Arc::new(Self {
            home: home.display().to_string(),
            scrub: parking_lot::RwLock::new(String::new()),
            state: Mutex::new(State::default()),
        })
    }

    /// A partir de ahora, `swarm` se sustituye por `<swarm>` en los eventos.
    pub fn set_scrub(&self, swarm: &str) {
        *self.scrub.write() = swarm.to_string();
    }

    /// `_stable`: rutas de la corrida y puertos locales fuera de la traza.
    pub fn stable(&self, value: Value) -> Value {
        match value {
            Value::String(text) => {
                let text = normalize_text(&text, &self.home, &self.home.replace('\\', "/"));
                let scrub = self.scrub.read().clone();
                Value::String(if scrub.is_empty() {
                    text
                } else {
                    text.replace(&scrub, "<swarm>")
                })
            }
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|item| self.stable(item)).collect())
            }
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, item)| (key, self.stable(item)))
                    .collect(),
            ),
            other => other,
        }
    }

    /// `record(event, **data)`: el `data` debe ser un objeto JSON.
    pub fn record(&self, event: &str, data: Value) {
        let data = self.stable(data);
        let mut state = self.state.lock();
        let key = current_key(&mut state);
        let seq = state.seq.entry(key.clone()).or_insert(0);
        *seq += 1;
        let mut item = Map::new();
        item.insert("thread".to_string(), Value::String(key));
        item.insert("seq".to_string(), Value::from(*seq));
        item.insert("event".to_string(), Value::String(event.to_string()));
        if let Value::Object(fields) = data {
            for (name, value) in fields {
                item.insert(name, value);
            }
        }
        state.events.push(Value::Object(item));
    }

    /// Registra un evento con la etiqueta de otro hilo del pipeline.
    ///
    /// El Python ejecuta el turno (y sus herramientas) en `ezagent-answer`; en
    /// Rust el mensajero lo hace en `ezagent-turn`. El arnés atribuye las
    /// herramientas al hilo de la respuesta para conservar la traza de Python;
    /// no consume un ordinal: usa el que tendrá el hilo de respuesta de este
    /// turno (el siguiente de su nombre).
    pub fn record_as(&self, name: &str, event: &str, data: Value) {
        let data = self.stable(data);
        let mut state = self.state.lock();
        let ordinal = *state.ordinals.get(name).unwrap_or(&0);
        let key = format!("{name}#{ordinal}");
        let seq = state.seq.entry(key.clone()).or_insert(0);
        *seq += 1;
        let mut item = Map::new();
        item.insert("thread".to_string(), Value::String(key));
        item.insert("seq".to_string(), Value::from(*seq));
        item.insert("event".to_string(), Value::String(event.to_string()));
        if let Value::Object(fields) = data {
            for (name, value) in fields {
                item.insert(name, value);
            }
        }
        state.events.push(Value::Object(item));
    }

    /// Eventos de un tipo, en orden de llegada.
    pub fn with_event(&self, event: &str) -> Vec<Value> {
        self.state
            .lock()
            .events
            .iter()
            .filter(|item| item.get("event").and_then(Value::as_str) == Some(event))
            .cloned()
            .collect()
    }

    /// Todas las eventos, en orden de llegada (el `chrono` del Python).
    pub fn events(&self) -> Vec<Value> {
        self.state.lock().events.clone()
    }

    /// Líneas del archivo: por hilo (orden de [`THREAD_ORDER`]) y por `seq`.
    pub fn lines(&self) -> Vec<String> {
        let state = self.state.lock();
        let mut ordered: Vec<&Value> = state.events.iter().collect();
        ordered.sort_by_key(|item| rank(item));
        ordered
            .into_iter()
            .map(|item| dumps_with(item, &DumpOptions::utf8()))
            .collect()
    }
}

/// `(rank, ordinal, seq)` de una línea.
fn rank(item: &Value) -> (usize, u64, u64) {
    let thread = item.get("thread").and_then(Value::as_str).unwrap_or("");
    let (name, ordinal) = thread
        .split_once('#')
        .map_or((thread, 0), |(name, ordinal)| {
            (name, ordinal.parse::<u64>().unwrap_or(0))
        });
    let rank = THREAD_ORDER
        .iter()
        .position(|known| *known == name)
        .unwrap_or(THREAD_ORDER.len());
    let seq = item.get("seq").and_then(Value::as_u64).unwrap_or(0);
    (rank, ordinal, seq)
}

/// Etiqueta del hilo actual (`<nombre>#<ordinal>`), asignada una vez.
fn current_key(state: &mut State) -> String {
    let thread = std::thread::current();
    let ident = thread.id();
    if let Some(key) = state.keys.get(&ident) {
        return key.clone();
    }
    let raw = thread.name().unwrap_or("");
    let name = match raw {
        "main" => "main",
        "" => "ezagent-call",
        other => other,
    };
    let ordinal = *state.ordinals.get(name).unwrap_or(&0);
    state.ordinals.insert(name.to_string(), ordinal + 1);
    let key = format!("{name}#{ordinal}");
    state.keys.insert(ident, key.clone());
    key
}

/// Sustituye la ruta de la corrida y los puertos `127.0.0.1:N`.
fn normalize_text(text: &str, home: &str, home_slash: &str) -> String {
    let mut value = text.replace(home, "<tmp>");
    if home_slash != home {
        value = value.replace(home_slash, "<tmp>");
    }
    scrub_ports(&value)
}

/// `re.sub(r"127\.0\.0\.1:\d+", "127.0.0.1:<port>", value)`.
fn scrub_ports(text: &str) -> String {
    const HOST: &str = "127.0.0.1:";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(HOST) {
        let (head, tail) = rest.split_at(index + HOST.len());
        out.push_str(head);
        let digits = tail.len() - tail.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 {
            out.push_str("<port>");
            rest = &tail[digits..];
        } else {
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ports_and_home_are_normalized_recursively() {
        let trace = Trace::new(Path::new(r"C:\tmp\ezparity-1"));
        let stable = trace.stable(json!({
            "url": "http://127.0.0.1:53100/v1",
            "path": r"C:\tmp\ezparity-1\hc.json",
            "nested": ["127.0.0.1:9 y 127.0.0.1:x"],
        }));
        assert_eq!(stable["url"], "http://127.0.0.1:<port>/v1");
        assert_eq!(stable["path"], "<tmp>\\hc.json");
        assert_eq!(stable["nested"][0], "127.0.0.1:<port> y 127.0.0.1:x");
    }

    #[test]
    fn lines_keep_the_sequence_of_their_thread() {
        let trace = Trace::new(Path::new("/tmp/x"));
        trace.record("a", json!({"value": 1}));
        trace.record("b", json!({}));
        let lines = trace.lines();
        assert_eq!(lines.len(), 2);
        let name = std::thread::current()
            .name()
            .unwrap_or("ezagent-call")
            .to_string();
        assert!(lines[0].contains(&format!("\"thread\": \"{name}#0\"")));
        assert!(lines[0].contains("\"seq\": 1"));
        assert!(lines[1].contains("\"seq\": 2"));
        assert_eq!(trace.with_event("b").len(), 1);
    }

    #[test]
    fn worker_threads_get_their_own_ordinal() {
        let trace = Trace::new(Path::new("/tmp/x"));
        let other = Arc::clone(&trace);
        std::thread::Builder::new()
            .name("ezagent-answer".to_string())
            .spawn(move || other.record("x", json!({})))
            .expect("hilo")
            .join()
            .expect("sin pánico");
        trace.record("y", json!({}));
        let lines = trace.lines();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|line| line.contains("ezagent-answer#0")));
    }
}
