//! Estado compartido del hub y los comandos que expone a la vista (§10).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::hub::{self, HubState};
use crate::input;
use crate::pty::{self, EventSink, PtyEvent, Session, Size};
use crate::runtime::{FishInfo, HubRuntime, RuntimeStatus};
use crate::selfcheck::{self, SelfcheckReport};
use crate::space::{self, Space, SpaceStore};
use crate::texts;
use crate::voicenote::VoiceNotes;

/// Estado que comparten todos los comandos de la ventana.
pub struct AppState {
    demo: bool,
    store: SpaceStore,
    spaces: Mutex<Vec<Space>>,
    views: Mutex<HashMap<String, Arc<Session>>>,
    elephants: Mutex<HashMap<String, String>>,
    hub: Mutex<HubState>,
    selfcheck_path: Option<PathBuf>,
    voice: Arc<VoiceNotes>,
    runtime: Option<Arc<HubRuntime>>,
}

impl AppState {
    /// Carga `espacios.json` o prepara el espacio de demostración.
    pub fn new(demo: bool) -> Self {
        Self::with_selfcheck(demo, None)
    }

    /// Como `new`, con la ruta donde la vista debe dejar su informe.
    pub fn with_selfcheck(demo: bool, selfcheck_path: Option<PathBuf>) -> Self {
        let store = SpaceStore::for_app();
        let (spaces, hub) = if demo {
            let folder = std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| ".".to_string());
            (vec![hub::demo_space(&folder)], hub::demo_state(&folder))
        } else {
            let spaces = match store.load() {
                Ok(spaces) => spaces,
                Err(error) => {
                    tracing::warn!(%error, "no pude leer espacios.json; empiezo sin espacios");
                    Vec::new()
                }
            };
            (spaces, HubState::default())
        };
        let data_dir = crate::dirs::app_data_dir().join("hub");
        let runtime = if demo {
            None
        } else {
            match HubRuntime::start(&data_dir) {
                Ok(runtime) => Some(Arc::new(runtime)),
                Err(error) => {
                    tracing::error!(%error, "no pude arrancar el motor del espacio de trabajo");
                    Some(Arc::new(HubRuntime::failed(&data_dir, &error)))
                }
            }
        };
        Self {
            demo,
            store,
            spaces: Mutex::new(spaces),
            views: Mutex::new(HashMap::new()),
            elephants: Mutex::new(HashMap::new()),
            hub: Mutex::new(hub),
            selfcheck_path,
            voice: Arc::new(VoiceNotes::new()),
            runtime,
        }
    }

    /// Motor del espacio de trabajo, si la ventana no está en demostración.
    pub fn runtime(&self) -> Option<Arc<HubRuntime>> {
        self.runtime.clone()
    }

    /// Estado del motor para la vista.
    pub fn runtime_status(&self) -> RuntimeStatus {
        if self.demo {
            return RuntimeStatus::demo();
        }
        match &self.runtime {
            Some(runtime) => runtime.status(),
            None => RuntimeStatus::degraded("El motor del espacio de trabajo no pudo arrancar."),
        }
    }

    /// Dirección hc del elefante del espacio (lo lanza si hace falta).
    pub fn space_elephant(&self, space: &str) -> Result<String, String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| "El motor del espacio de trabajo no está disponible.".to_string())?
            .ensure_elephant(space)
    }

    /// Peces vivos del espacio.
    pub fn space_fish(&self, space: &str) -> Result<Vec<FishInfo>, String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| "El motor del espacio de trabajo no está disponible.".to_string())?
            .space_fish(space)
    }

    /// Envía texto crudo a la terminal del elefante.
    pub fn elephant_send(&self, space: &str, text: &str) -> Result<(), String> {
        self.runtime
            .as_ref()
            .ok_or_else(|| "El motor del espacio de trabajo no está disponible.".to_string())?
            .elephant_send(space, text)
    }

    /// Ruta del informe de autocomprobación, si está activa.
    pub fn selfcheck_path(&self) -> Option<&Path> {
        self.selfcheck_path.as_deref()
    }

    /// Escribe el informe fuera de candados y devuelve si pasó la evaluación.
    pub fn submit_selfcheck(&self, report: &SelfcheckReport) -> Result<bool, String> {
        let path = self
            .selfcheck_path()
            .ok_or_else(|| "La autocomprobación no está activa.".to_string())?;
        selfcheck::write_json(path, report)?;
        Ok(selfcheck::evaluate(report))
    }

    /// Empieza la nota de voz de la barra.
    pub fn voice_start(&self, app: AppHandle) -> Result<(), String> {
        self.voice.start(app)
    }

    /// Para la nota y devuelve el texto transcrito.
    pub fn voice_stop(&self) -> Result<String, String> {
        self.voice.stop()
    }

    /// Inserta el texto dictado en el cursor del área de entrada.
    pub fn voice_insert(&self, current: &str, cursor: usize, added: &str) -> (String, usize) {
        self.voice.insert(current, cursor, added)
    }

    /// Espacios guardados (o el de demostración, en memoria).
    pub fn spaces(&self) -> Vec<Space> {
        self.spaces.lock().clone()
    }

    /// Crea un espacio y lo guarda; en demostración solo vive en memoria.
    pub fn create_space(&self, nombre: &str, carpeta: &str, agente: &str) -> Result<Space, String> {
        let space = space::create_space(nombre, carpeta, agente)?;
        let mut next = self.spaces.lock().clone();
        next.push(space.clone());
        // DECISION: la escritura va fuera del candado; si falla, la lista en memoria no cambia.
        if !self.demo {
            self.store
                .save(&next)
                .map_err(|error| format!("{} {error}", texts::SPACES_FILE))?;
        }
        *self.spaces.lock() = next;
        Ok(space)
    }

    /// Abre una terminal y devuelve su id. Con `space` la registra como elefante.
    pub fn open_view(
        &self,
        argv: Vec<String>,
        cwd: Option<String>,
        size: Size,
        channel: Channel<InvokeResponseBody>,
        space: Option<String>,
    ) -> Result<String, String> {
        let id = Uuid::new_v4().to_string();
        let sink: EventSink = Arc::new(move |event| {
            let body = match event {
                PtyEvent::Data(bytes) => InvokeResponseBody::Raw(bytes),
                PtyEvent::Exited(code) => {
                    let payload = serde_json::json!({ "event": "exit", "code": code });
                    match serde_json::to_string(&payload) {
                        Ok(json) => InvokeResponseBody::Json(json),
                        Err(error) => {
                            tracing::warn!(%error, "no pude serializar el aviso de salida");
                            return;
                        }
                    }
                }
            };
            if let Err(error) = channel.send(body) {
                tracing::debug!(%error, "la vista ya no escucha");
            }
        });
        let cwd_path = cwd.as_deref().map(PathBuf::from);
        let session = pty::open(&id, &argv, cwd_path.as_deref(), &[], size, sink)
            .map_err(|error| error.to_string())?;
        self.views.lock().insert(id.clone(), Arc::new(session));
        if let Some(space) = space {
            self.elephants.lock().insert(space, id.clone());
        }
        Ok(id)
    }

    /// Escribe en una terminal; la sesión se suelta antes de encolar los bytes.
    pub fn write_view(&self, id: &str, data: &[u8]) -> Result<(), String> {
        let session = self.session(id)?;
        session.write(data).map_err(|error| error.to_string())
    }

    /// Redimensiona una terminal.
    pub fn resize_view(&self, id: &str, size: Size) -> Result<(), String> {
        let session = self.session(id)?;
        session.resize(size).map_err(|error| error.to_string())
    }

    /// Cierra y olvida una terminal.
    pub fn close_view(&self, id: &str) {
        let session = self.views.lock().remove(id);
        self.elephants.lock().retain(|_, view| view != id);
        if let Some(session) = session {
            session.close();
        }
    }

    fn session(&self, id: &str) -> Result<Arc<Session>, String> {
        let views = self.views.lock();
        views
            .get(id)
            .cloned()
            .ok_or_else(|| texts::unknown_view(id))
    }

    /// Envía el texto de la barra a la terminal del elefante de ese espacio.
    pub fn elephant_input(&self, space: &str, text: &str) -> Result<(), String> {
        let view = {
            let elephants = self.elephants.lock();
            elephants
                .get(space)
                .cloned()
                .ok_or_else(|| texts::ELEPHANT_CLOSED.to_string())?
        };
        self.write_view(&view, &input::encode_input(text))
    }

    /// Peces, traza y tarjetas del espacio.
    pub fn hub_state(&self, space: &str) -> HubState {
        if self.demo && space == hub::DEMO_SPACE_ID {
            return self.hub.lock().clone();
        }
        HubState::default()
    }

    /// Resuelve una tarjeta; rechazar exige comentario.
    pub fn card_action(
        &self,
        card: &str,
        action: &str,
        comment: Option<&str>,
    ) -> Result<(), String> {
        match action {
            "approve" => {}
            "reject" => {
                if comment.map(str::trim).unwrap_or_default().is_empty() {
                    return Err(texts::CARD_COMMENT_REQUIRED.to_string());
                }
            }
            _ => return Err(texts::CARD_ACTION_INVALID.to_string()),
        }
        let mut hub = self.hub.lock();
        let before = hub.cards.len();
        hub.cards.retain(|item| item.id != card);
        if hub.cards.len() == before {
            return Err(texts::unknown_card(card));
        }
        drop(hub);
        tracing::info!(
            card,
            action,
            comment = comment.unwrap_or_default(),
            "tarjeta resuelta"
        );
        Ok(())
    }
}

/// Espacios guardados en `espacios.json`.
#[tauri::command]
pub fn spaces_list(state: State<'_, AppState>) -> Vec<Space> {
    state.spaces()
}

/// Crea un espacio con nombre, carpeta existente y agente.
#[tauri::command]
pub fn space_create(
    state: State<'_, AppState>,
    nombre: String,
    carpeta: String,
    agente: String,
) -> Result<Space, String> {
    state.create_space(&nombre, &carpeta, &agente)
}

/// Abre una vista PTY. `space` la registra como la terminal del elefante.
// DECISION: el brief no da forma de asociar `elephant_input {space, text}` con la vista
// del elefante; `view_open` acepta `space` opcional para hacer esa asociación.
#[tauri::command]
pub fn view_open(
    state: State<'_, AppState>,
    argv: Vec<String>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    channel: Channel<InvokeResponseBody>,
    space: Option<String>,
) -> Result<String, String> {
    state.open_view(argv, cwd, Size { cols, rows }, channel, space)
}

/// Escribe en la vista.
#[tauri::command]
pub fn view_write(state: State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    state.write_view(&id, data.as_bytes())
}

/// Redimensiona la vista.
#[tauri::command]
pub fn view_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.resize_view(&id, Size { cols, rows })
}

/// Mata el proceso y olvida la vista.
#[tauri::command]
pub fn view_close(state: State<'_, AppState>, id: String) {
    state.close_view(&id);
}

/// Manda el texto de la barra al elefante, codificado como pegado si tiene saltos.
#[tauri::command]
pub fn elephant_input(
    state: State<'_, AppState>,
    space: String,
    text: String,
) -> Result<(), String> {
    state.elephant_input(&space, &text)
}

/// Peces, traza y tarjetas del espacio.
#[tauri::command]
pub fn hub_state(state: State<'_, AppState>, space: String) -> HubState {
    state.hub_state(&space)
}

/// Aprueba o rechaza una tarjeta y avisa a la vista.
#[tauri::command]
pub fn card_action(
    app: AppHandle,
    state: State<'_, AppState>,
    card: String,
    action: String,
    comment: Option<String>,
) -> Result<(), String> {
    state.card_action(&card, &action, comment.as_deref())?;
    if let Err(error) = app.emit("hub://changed", ()) {
        tracing::warn!(%error, "no pude avisar a la vista");
    }
    Ok(())
}

/// Estado del motor del espacio de trabajo (normal, degradado o demo).
#[tauri::command]
pub fn runtime_status(state: State<'_, AppState>) -> RuntimeStatus {
    state.runtime_status()
}

/// Dirección hc del elefante del espacio; lo lanza si no está vivo.
#[tauri::command]
pub fn space_elephant(state: State<'_, AppState>, space: String) -> Result<String, String> {
    state.space_elephant(&space)
}

/// Peces vivos del espacio con dirección, papel, agente, modelo y estado.
#[tauri::command]
pub fn space_fish(state: State<'_, AppState>, space: String) -> Result<Vec<FishInfo>, String> {
    state.space_fish(&space)
}

/// Envía el texto de la barra al elefante por herdr-core.
#[tauri::command]
pub fn elephant_send(
    state: State<'_, AppState>,
    space: String,
    text: String,
) -> Result<(), String> {
    state.elephant_send(&space, &text)
}

/// Texto transcrito por la nota de voz.
#[derive(Debug, Serialize)]
pub struct VoiceText {
    pub texto: String,
}

/// Resultado de insertar el texto en el cursor del área de entrada.
#[derive(Debug, Serialize)]
pub struct InsertResult {
    pub text: String,
    pub cursor: usize,
}

/// Empieza a grabar la nota de voz; el nivel se emite en `hub://voice-level`.
#[tauri::command]
pub fn voice_note_start(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.voice_start(app)
}

/// Para la grabación y devuelve `{ "texto": ... }` sin enviarlo al elefante.
#[tauri::command]
pub fn voice_note_stop(state: State<'_, AppState>) -> Result<VoiceText, String> {
    state.voice_stop().map(|texto| VoiceText { texto })
}

/// Inserta el texto dictado en `cursor` y devuelve el texto y el cursor nuevo.
#[tauri::command]
pub fn voice_insert(
    state: State<'_, AppState>,
    current: String,
    cursor: usize,
    added: String,
) -> InsertResult {
    let (text, cursor) = state.voice_insert(&current, cursor, &added);
    InsertResult { text, cursor }
}

/// La vista pregunta si debe enviar el informe de autocomprobación.
#[tauri::command]
pub fn selfcheck_enabled(state: State<'_, AppState>) -> bool {
    state.selfcheck_path().is_some()
}

/// Recibe el informe de la vista, lo guarda y cierra la app con su código de salida.
#[tauri::command]
pub fn selfcheck_report(
    app: AppHandle,
    state: State<'_, AppState>,
    report: SelfcheckReport,
) -> Result<(), String> {
    let passed = state.submit_selfcheck(&report)?;
    tracing::info!(passed, "informe de autocomprobación recibido");
    app.exit(if passed { 0 } else { 1 });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn demo_opens_with_the_memory_space() {
        let state = AppState::new(true);
        let spaces = state.spaces();
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0].id, hub::DEMO_SPACE_ID);
        assert_eq!(state.hub_state(hub::DEMO_SPACE_ID).fish.len(), 2);
    }

    #[test]
    fn demo_state_does_not_touch_espacios_json() {
        let state = AppState::new(true);
        let before = std::fs::metadata(state.store.path())
            .and_then(|meta| meta.modified())
            .ok();
        let dir = TempDir::new("demo-create");
        let space = state
            .create_space("Nuevo", &dir.path().display().to_string(), "claude")
            .expect("crear en memoria");
        let after = std::fs::metadata(state.store.path())
            .and_then(|meta| meta.modified())
            .ok();
        assert_eq!(state.spaces().len(), 2);
        assert_eq!(before, after, "el demo no debe tocar espacios.json");
        assert_eq!(space.elephant.agent, "claude");
    }

    #[test]
    fn approve_removes_the_card_and_reject_requires_a_comment() {
        let state = AppState::new(true);
        assert!(state.card_action("equipo", "reject", Some("  ")).is_err());
        assert!(state.card_action("equipo", "otra", None).is_err());
        state
            .card_action("equipo", "approve", None)
            .expect("aprobar");
        assert!(state.hub_state(hub::DEMO_SPACE_ID).cards.is_empty());
        assert!(state.card_action("equipo", "approve", None).is_err());
    }

    #[test]
    fn unknown_space_has_an_empty_state() {
        let state = AppState::new(true);
        let other = state.hub_state("otro");
        assert!(other.fish.is_empty() && other.timeline.is_empty() && other.cards.is_empty());
    }

    #[test]
    fn writing_to_an_unknown_view_is_an_error_with_a_message() {
        let state = AppState::new(true);
        let error = state
            .write_view("no-existe", b"x")
            .expect_err("debe fallar");
        assert!(error.contains("no-existe"));
    }

    #[test]
    fn elephant_input_needs_an_open_terminal() {
        let state = AppState::new(true);
        assert!(state.elephant_input(hub::DEMO_SPACE_ID, "hola").is_err());
    }

    #[test]
    fn selfcheck_writes_the_report_and_reports_the_verdict() {
        let dir = TempDir::new("demo-selfcheck");
        let path = dir.path().join("informe.json");
        let state = AppState::with_selfcheck(true, Some(path.clone()));
        assert!(state.selfcheck_path().is_some());
        let report = SelfcheckReport {
            cabecera: "Agente principal · PowerShell".to_string(),
            terminales: 3,
            peces: 2,
            eventos: 5,
            tarjetas: 1,
            ..SelfcheckReport::default()
        };
        assert!(state.submit_selfcheck(&report).expect("informe"));
        let text = std::fs::read_to_string(&path).expect("leer informe");
        let saved: SelfcheckReport = serde_json::from_str(&text).expect("json");
        assert_eq!(saved, report);
        let bad = SelfcheckReport {
            terminales: 1,
            ..report
        };
        assert!(!state.submit_selfcheck(&bad).expect("informe malo"));
    }

    #[test]
    fn selfcheck_is_off_by_default() {
        let state = AppState::new(true);
        assert!(state.selfcheck_path().is_none());
        assert!(state.submit_selfcheck(&SelfcheckReport::default()).is_err());
    }

    #[test]
    fn voice_insert_adds_a_separator_at_the_cursor() {
        let state = AppState::new(true);
        let (text, cursor) = state.voice_insert("hola", 4, "mundo");
        assert_eq!(text, "hola mundo");
        assert_eq!(cursor, 10);
    }

    #[test]
    fn stopping_a_voice_note_that_never_started_is_an_error() {
        let state = AppState::new(true);
        assert!(state.voice_stop().is_err());
    }
}
