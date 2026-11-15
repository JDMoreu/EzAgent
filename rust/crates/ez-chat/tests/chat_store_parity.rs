//! Paridad de `voice/chat/store.py`, `session.py` y `contexts.py` contra
//! `parity/fixtures/chat_store.jsonl`. Un solo `#[test]` porque los escenarios
//! tocan el disco y el entorno y comparten el reloj congelado del oráculo.
use ez_chat::contexts::{ChatContexts, ContextHolder, MAX_SAVED};
use ez_chat::session::{ChatMessage, ChatModel};
use ez_chat::store::{
    as_messages, default_chat_path, seq_of, title, title_with, ChatEntry, ChatStore, Clock,
    ModelMessage, Role, Source, MAX_ENTRY_CHARS, VERSION,
};
use ez_core::pyvalue::{python_str, text_or_empty, truthy};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
const FIXTURES: &str = "../../../parity/fixtures/chat_store.jsonl";
fn clock() -> Clock {
    Arc::new(|| 1_735_689_600.0)
}
fn role_of(name: &str) -> Role {
    match name {
        "assistant" => Role::Assistant,
        "system" => Role::System,
        _ => Role::User,
    }
}
fn source_of(name: &str) -> Source {
    match name {
        "chat" => Source::Chat,
        "voice" => Source::Voice,
        "agent" => Source::Agent,
        other => Source::Other(other.to_string()),
    }
}
fn op_s<'a>(op: &'a Value, key: &str, default: &'a str) -> &'a str {
    op.get(key).and_then(Value::as_str).unwrap_or(default)
}
fn op_i(op: &Value, key: &str, default: i64) -> i64 {
    op.get(key).and_then(Value::as_i64).unwrap_or(default)
}
fn op_b(op: &Value, key: &str) -> bool {
    op.get(key).is_some_and(truthy)
}
fn opt_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Null => None,
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
fn entry_json(entry: &ChatEntry) -> Value {
    json!({"t": entry.t, "session": entry.session, "role": entry.role.as_str(),
        "source": entry.source.as_str(), "text": entry.text, "seq": entry.seq})
}
fn array<T>(items: &[T], f: fn(&T) -> Value) -> Value {
    Value::Array(items.iter().map(f).collect())
}
fn sessions_json(infos: &[ez_chat::store::SessionInfo]) -> Value {
    array(infos, |i| {
        json!({"session": i.session, "started": i.started,
        "messages": i.messages, "title": i.title})
    })
}
fn message_json(message: &ChatMessage) -> Value {
    json!({"id": message.id, "role": message.role.as_str(), "text": message.text,
        "source": message.source.as_str(), "t": message.t,
        "streaming": message.streaming, "error": message.error})
}
fn file_text(path: &Path) -> Value {
    match (path.is_dir(), path.exists()) {
        (true, _) => json!("<dir>"),
        (false, true) => json!(String::from_utf8_lossy(&fs::read(path).expect("archivo"))),
        _ => Value::Null,
    }
}
fn work(root: &Path, group: &str, case: &FixtureCase) -> PathBuf {
    root.join(group)
        .join(case.case.replace('/', "-"))
        .join("chat.jsonl")
}
fn write_seed(path: &Path, seed: Option<&Value>) {
    let Some(text) = seed.and_then(Value::as_str) else {
        return;
    };
    if text == "<dir>" {
        return fs::create_dir_all(path).expect("carpeta del caso");
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("carpeta del caso");
    }
    fs::write(path, text.as_bytes()).expect("archivo del caso");
}
struct StoreRun {
    path: PathBuf,
    store: ChatStore,
    results: Vec<Value>,
}
impl StoreRun {
    fn new(path: PathBuf, seed: Option<&Value>) -> Self {
        write_seed(&path, seed);
        let store = ChatStore::with_clock(Some(path.clone()), clock());
        Self {
            path,
            store,
            results: Vec::new(),
        }
    }
    fn run(&mut self, op: &Value) -> Value {
        let result = match op_s(op, "op", "") {
            "append" => entry_json(&self.store.append(
                role_of(op_s(op, "role", "user")),
                &text_or_empty(op.get("text")),
                source_of(op_s(op, "source", "chat")),
                opt_f64(op.get("t")),
            )),
            "replace_last" => json!(self.store.replace_last(
                role_of(op_s(op, "role", "user")),
                &text_or_empty(op.get("old")),
                &text_or_empty(op.get("new")),
            )),
            "new_session" => json!(self.store.new_session()),
            "resume" => json!(self.store.resume(op_i(op, "session", 0))),
            "delete_session" => json!(self.store.delete_session(op_i(op, "session", 0))),
            "entries" => array(
                &self
                    .store
                    .entries(op.get("session").and_then(Value::as_i64)),
                entry_json,
            ),
            "recent" => array(&self.store.recent(op_i(op, "limit", 200)), entry_json),
            "sessions" => sessions_json(&self.store.sessions()),
            "search" => array(
                &self
                    .store
                    .search(&text_or_empty(op.get("query")), op_i(op, "limit", 50)),
                entry_json,
            ),
            "len" => json!(self.store.len()),
            "reload" => {
                self.store = ChatStore::with_clock(Some(self.path.clone()), clock());
                json!({"session": self.store.session(), "len": self.store.len()})
            }
            "write_raw" => {
                if let Some(parent) = self.path.parent() {
                    fs::create_dir_all(parent).expect("carpeta");
                }
                let text = text_or_empty(op.get("text"));
                fs::write(&self.path, text.as_bytes()).expect("escritura cruda");
                json!(true)
            }
            "block_path" => {
                if self.path.is_file() {
                    fs::rename(&self.path, self.path.with_file_name("chat.jsonl.old"))
                        .expect("apartar el archivo");
                }
                fs::create_dir_all(&self.path).expect("bloquear la ruta");
                json!(true)
            }
            "unblock_path" => {
                if self.path.is_dir() {
                    fs::remove_dir(&self.path).expect("liberar la ruta");
                }
                let old = self.path.with_file_name("chat.jsonl.old");
                if old.exists() {
                    fs::rename(&old, &self.path).expect("restaurar el archivo");
                }
                json!(true)
            }
            other => panic!("operación de ChatStore desconocida: {other}"),
        };
        self.results.push(result.clone());
        result
    }
    fn finish(&self) -> Value {
        let state = json!({
            "session": self.store.session(), "len": self.store.len(),
            "sessions": sessions_json(&self.store.sessions()),
            "entries": array(&self.store.entries(None), entry_json),
            "recent": array(&self.store.recent(200), entry_json),
            "file": file_text(&self.path),
        });
        json!({"results": self.results, "state": state})
    }
}
fn run_setup(chat: &ChatModel, setup: &[Value]) {
    for op in setup {
        if let Some(text) = op.get("add_user").and_then(Value::as_str) {
            chat.add_user(text, source_of(op_s(op, "source", "chat")));
        } else if op.get("new_session").is_some() {
            chat.new_session();
        } else if op.get("begin_assistant").is_some() {
            chat.begin_assistant(source_of(op_s(op, "source", "chat")));
        }
    }
}
struct SessionRun {
    chat: Arc<ChatModel>,
    path: PathBuf,
    results: Vec<Value>,
    ids: Vec<Option<i64>>,
}
impl SessionRun {
    fn new(path: PathBuf, spec: &Value) -> Self {
        write_seed(&path, spec.get("seed"));
        let store = spec
            .get("store")
            .is_none_or(truthy)
            .then(|| Arc::new(ChatStore::with_clock(Some(path.clone()), clock())));
        let chat = Arc::new(ChatModel::with_clock(
            store,
            op_b(spec, "voice_replies"),
            clock(),
        ));
        let setup = spec
            .get("setup")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        run_setup(&chat, &setup);
        Self {
            chat,
            path,
            results: Vec::new(),
            ids: Vec::new(),
        }
    }
    fn id_of(&self, value: &Value) -> i64 {
        if let Some(rest) = value.as_str().and_then(|text| text.strip_prefix('$')) {
            return self.ids[rest.parse::<usize>().expect("índice de id")].unwrap_or(0);
        }
        value.as_i64().expect("id del caso")
    }
    fn run(&mut self, op: &Value) -> Value {
        let result = match op_s(op, "op", "") {
            "add_user" => message_json(&self.chat.add_user(
                &text_or_empty(op.get("text")),
                source_of(op_s(op, "source", "chat")),
            )),
            "revise_user" => json!(self.chat.revise_user(
                self.id_of(op.get("id").unwrap_or(&Value::Null)),
                &text_or_empty(op.get("text")),
            )),
            "add_note" => message_json(&self.chat.add_note(&text_or_empty(op.get("text")))),
            "begin_assistant" => message_json(
                &self
                    .chat
                    .begin_assistant(source_of(op_s(op, "source", "chat"))),
            ),
            "append" | "finish" | "fail" | "cancel" => {
                let id = self.id_of(op.get("id").unwrap_or(&Value::Null));
                match op_s(op, "op", "") {
                    "append" => self.chat.append(id, &text_or_empty(op.get("delta"))),
                    "finish" => self.chat.finish(id),
                    "fail" => self.chat.fail(id, &text_or_empty(op.get("text"))),
                    _ => self.chat.cancel(id),
                }
                Value::Null
            }
            "ask_confirm" => {
                self.chat
                    .ask_confirm(&text_or_empty(op.get("summary")), op_b(op, "strong"));
                Value::Null
            }
            "clear_confirm" => {
                self.chat.clear_confirm();
                Value::Null
            }
            "new_session" => {
                self.chat.new_session();
                json!(self.chat.current_session())
            }
            "switch_session" => json!(self.chat.switch_session(op_i(op, "session", 0))),
            "delete_session" => json!(self.chat.delete_session(op_i(op, "session", 0))),
            "view_session" => {
                self.chat
                    .view_session(op.get("session").and_then(Value::as_i64));
                json!(self.chat.viewing())
            }
            "set_voice_replies" => {
                self.chat.set_voice_replies(op_b(op, "enabled"));
                json!(self.chat.voice_replies())
            }
            "set_mic_muted" => {
                self.chat.set_mic_muted(op_b(op, "muted"));
                json!(self.chat.mic_muted())
            }
            "last_assistant" => json!(self.chat.last_assistant_text()),
            "sessions" => sessions_json(&self.chat.sessions()),
            "snapshot" => {
                let (version, messages) = self.chat.snapshot();
                json!({"version": version, "messages": array(&messages, message_json)})
            }
            "state" => state_json(&self.chat),
            other => panic!("operación de ChatModel desconocida: {other}"),
        };
        self.results.push(result.clone());
        self.ids.push(result.get("id").and_then(Value::as_i64));
        result
    }
    fn finish(&self) -> Value {
        let mut state = state_json(&self.chat);
        state["sessions"] = sessions_json(&self.chat.sessions());
        let (version, messages) = self.chat.snapshot();
        state["snapshot"] = json!({"version": version, "messages": array(&messages, message_json)});
        json!({"results": self.results, "state": state, "file": file_text(&self.path)})
    }
}
fn state_json(chat: &ChatModel) -> Value {
    json!({
        "version": chat.version(), "busy": chat.busy(),
        "confirm": chat.confirm().map_or(Value::Null, |card| {
            json!({"summary": card.summary, "strong": card.strong})
        }),
        "viewing": chat.viewing(), "current_session": chat.current_session(),
        "voice_replies": chat.voice_replies(), "mic_muted": chat.mic_muted(),
    })
}
#[derive(Default)]
struct FakeHolder {
    calls: Mutex<Vec<Value>>,
    exports: Map<String, Value>,
    session: Mutex<String>,
}
fn model_message(value: &Value) -> ModelMessage {
    ModelMessage {
        role: role_of(op_s(value, "role", "user")),
        content: op_s(value, "content", "").to_string(),
    }
}
fn model_json(message: &ModelMessage) -> Value {
    json!({"role": message.role.as_str(), "content": message.content})
}
impl ContextHolder for FakeHolder {
    fn reset_history(&self) {
        self.calls
            .lock()
            .expect("lock")
            .push(json!({"call": "reset"}));
    }
    fn export_context(&self) -> (Vec<ModelMessage>, String, String) {
        let session = self.session.lock().expect("lock").clone();
        let data = self.exports.get(&session).or_else(|| self.exports.get("*"));
        let data = data.cloned().unwrap_or_else(|| json!({}));
        self.calls
            .lock()
            .expect("lock")
            .push(json!({"call": "export", "session": session}));
        let messages = data
            .get("messages")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(model_message).collect())
            .unwrap_or_default();
        (
            messages,
            op_s(&data, "summary", "").to_string(),
            op_s(&data, "last_user", "").to_string(),
        )
    }
    fn import_context(&self, messages: Vec<ModelMessage>, summary: &str, last_user: &str) {
        let messages: Vec<Value> = messages.iter().map(model_json).collect();
        self.calls.lock().expect("lock").push(json!({
            "call": "import", "messages": messages,
            "summary": summary, "last_user": last_user,
        }));
    }
}
fn holder_source(
    chat: &Arc<ChatModel>,
    holder: &Arc<FakeHolder>,
) -> Box<dyn Fn() -> Option<Arc<dyn ContextHolder>> + Send + Sync> {
    let weak = Arc::downgrade(chat);
    let strong = Arc::clone(holder);
    Box::new(move || {
        let chat = weak.upgrade()?;
        let session = chat
            .current_session()
            .map_or_else(String::new, |v| v.to_string());
        *strong.session.lock().expect("lock") = session;
        Some(Arc::clone(&strong) as Arc<dyn ContextHolder>)
    })
}
struct ContextRun {
    chat: Arc<ChatModel>,
    contexts: ChatContexts,
    holder: Arc<FakeHolder>,
    path: PathBuf,
    results: Vec<Value>,
}
/// `_ContextRun.finish` no copia `seed` al `input`; se recompone como lo escribió el oráculo.
// DECISION: el seed es dato ausente del fixture, no un valor inventado.
const CONTEXT_SEEDS: [(&str, &str); 1] = [(
    "chat_store/contexts/rebuild",
    "{\"v\": 1, \"t\": 1735689600.0, \"session\": 1, \"role\": \"user\", \"text\": \"guardado\", \"seq\": 1}\n",
)];
impl ContextRun {
    fn new(path: PathBuf, spec: &Value, name: &str) -> Self {
        let seed = spec.get("seed").cloned().or_else(|| {
            CONTEXT_SEEDS
                .iter()
                .find(|(case, _)| *case == name)
                .map(|(_, text)| Value::String((*text).to_string()))
        });
        write_seed(&path, seed.as_ref());
        let store = spec
            .get("store")
            .is_none_or(truthy)
            .then(|| Arc::new(ChatStore::with_clock(Some(path.clone()), clock())));
        let chat = Arc::new(ChatModel::with_clock(store, false, clock()));
        let setup = spec
            .get("setup")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        run_setup(&chat, &setup);
        let exports = spec
            .get("exports")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let holder = Arc::new(FakeHolder {
            calls: Mutex::new(Vec::new()),
            exports,
            session: Mutex::new(String::new()),
        });
        let contexts = if op_s(spec, "holder", "fake") == "none" {
            ChatContexts::new(Arc::clone(&chat), Box::new(|| None))
        } else {
            ChatContexts::new(Arc::clone(&chat), holder_source(&chat, &holder))
        };
        Self {
            chat,
            contexts,
            holder,
            path,
            results: Vec::new(),
        }
    }
    fn saved(&self) -> Value {
        Value::Array(
            self.contexts
                .saved
                .iter()
                .map(|(session, saved)| {
                    json!({
                        "session": session,
                        "messages": saved.messages.iter().map(model_json).collect::<Vec<_>>(),
                        "summary": saved.summary, "last_user": saved.last_user,
                    })
                })
                .collect(),
        )
    }
    fn state(&self) -> Value {
        let (version, messages) = self.chat.snapshot();
        json!({
            "session": self.chat.current_session(), "version": version,
            "busy": self.chat.busy(), "messages": array(&messages, message_json),
            "sessions": sessions_json(&self.chat.sessions()),
        })
    }
    fn run(&mut self, op: &Value) -> Value {
        match op_s(op, "op", "") {
            "new_chat" => {
                self.contexts.new_chat();
                json!(true)
            }
            "switch" => json!(self.contexts.switch(op_i(op, "session", 0))),
            "delete" => json!(self.contexts.delete(op_i(op, "session", 0))),
            "rebuild" => {
                self.contexts
                    .rebuild(op.get("session").and_then(Value::as_i64));
                json!(true)
            }
            "saved" => self.saved(),
            "state" => self.state(),
            other => panic!("operación de ChatContexts desconocida: {other}"),
        }
    }
    fn finish(&self) -> Value {
        json!({
            "results": self.results, "calls": self.holder.calls.lock().expect("lock").clone(),
            "saved": self.saved(), "state": self.state(), "file": file_text(&self.path),
        })
    }
}
fn as_messages_case(case: &FixtureCase) -> Value {
    let entries: Vec<ChatEntry> = case.input["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|item| ChatEntry {
            t: item.get("t").and_then(Value::as_f64).unwrap_or(0.0),
            session: op_i(item, "session", 0),
            role: role_of(op_s(item, "role", "user")),
            source: source_of(op_s(item, "source", "chat")),
            text: text_or_empty(item.get("text")),
            seq: op_i(item, "seq", 0),
        })
        .collect();
    let messages: Vec<Value> = as_messages(&entries).iter().map(model_json).collect();
    json!({"messages": messages})
}
fn marked(text: &str, root: &Path) -> String {
    text.replace("<tmp>", &root.to_string_lossy())
}
fn norm_path(path: &Path, root: &Path) -> String {
    let root = root.to_string_lossy().replace('\\', "/");
    path.to_string_lossy()
        .replace('\\', "/")
        .replace(&root, "<tmp>")
}
fn set_env(name: &str, value: Option<&Value>, root: &Path) {
    match value.and_then(Value::as_str) {
        Some(text) => std::env::set_var(name, marked(text, root)),
        None => std::env::remove_var(name),
    }
}
fn default_path_case(case: &FixtureCase, root: &Path) -> Value {
    set_env("APPDATA", case.input.get("APPDATA"), root);
    set_env("XDG_CONFIG_HOME", case.input.get("XDG_CONFIG_HOME"), root);
    json!({"path": norm_path(&default_chat_path(), root)})
}
#[test]
fn every_chat_store_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de chat_store legibles");
    let root = std::env::temp_dir().join(format!("ez-chat-parity-{}", std::process::id()));
    fs::create_dir_all(&root).expect("raíz temporal");
    let mut cases_run = 0;
    for case in &cases {
        assert_eq!(case.error, None, "caso {} con error", case.case);
        match case.function.as_str() {
            "ChatStore" => {
                let mut run = StoreRun::new(work(&root, "store", case), case.input.get("seed"));
                for op in case.input["ops"].as_array().expect("ops") {
                    run.run(op);
                }
                assert_eq!(run.finish(), case.output, "caso {}", case.case);
            }
            "ChatModel" => {
                let mut run = SessionRun::new(work(&root, "session", case), &case.input);
                for op in case.input["ops"].as_array().expect("ops") {
                    run.run(op);
                }
                assert_eq!(run.finish(), case.output, "caso {}", case.case);
            }
            "ChatContexts" => {
                let mut run =
                    ContextRun::new(work(&root, "contexts", case), &case.input, &case.case);
                for op in case.input["ops"].as_array().expect("ops") {
                    let result = run.run(op);
                    run.results.push(result);
                }
                assert_eq!(run.finish(), case.output, "caso {}", case.case);
            }
            "title" => {
                let text = case.input.get("text").map_or_else(String::new, python_str);
                let limit = case.input.get("limit").and_then(Value::as_u64);
                let produced =
                    limit.map_or_else(|| title(&text), |limit| title_with(&text, limit as usize));
                assert_eq!(
                    json!({"title": produced}),
                    case.output,
                    "caso {}",
                    case.case
                );
            }
            "as_messages" => {
                assert_eq!(as_messages_case(case), case.output, "caso {}", case.case);
            }
            "seq_of" => {
                let index = case.input["index"].as_i64().expect("index");
                let produced = seq_of(case.input.get("data").expect("data"), index);
                assert_eq!(json!({"seq": produced}), case.output, "caso {}", case.case);
            }
            "default_chat_path" => {
                assert_eq!(
                    default_path_case(case, &root),
                    case.output,
                    "caso {}",
                    case.case
                );
            }
            "module_constants" => {
                let constants = json!({
                    "max_entry_chars": MAX_ENTRY_CHARS,
                    "version": VERSION, "max_saved": MAX_SAVED,
                });
                assert_eq!(constants, case.output, "caso {}", case.case);
            }
            other => panic!("función desconocida en {}: {other}", case.case),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 85, "muy pocos casos: {cases_in_file}");
}
