//! Vueltas del turno del `Messenger` (F4-07/F4-08).
//!
//! Prepara el historial y el prompt, alterna respuestas del modelo y ejecución
//! de herramientas, y deja el historial válido si el turno se corta.

use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_core::llm_providers::ModelCapabilities;
use ez_core::pyvalue::{py_strip, text_or_empty};
use serde_json::{json, Value};

use crate::context;
use crate::errors::LlmError;
use crate::tool_json::{repair_structure, sanitize_history};
use crate::tool_markup::{ToolMarkupGuard, TOOL_MARKUP_NOTICE};

use super::super::request::{remaining_tools, seal_tool_calls};
use super::super::{
    lock, ActiveStream, AgentRuntimeApi, AskMode, Messenger, TurnError, TurnToken,
    CHAT_MODE_PROMPT, EMPTY_ANSWER, LENGTH_NOTICE, TURN_TIMEOUT_TEXT,
};
use super::execution::{append_text, call_arguments, call_name};
use super::tool_ack;

/// Estado acumulado entre vueltas de un turno.
struct TurnState {
    partial: Vec<String>,
    open_calls: Vec<Value>,
    assistant_appended: bool,
    completed: bool,
    spoke: bool,
    finish_turn: bool,
}

impl TurnState {
    fn new(chat: bool) -> Self {
        Self {
            partial: Vec::new(),
            open_calls: Vec::new(),
            assistant_appended: false,
            completed: false,
            // El acuse de relleno es para la voz; en el chat no hace falta.
            spoke: chat,
            finish_turn: false,
        }
    }
}

/// Lo que necesita una vuelta del bucle, ya resuelto al empezar el turno.
struct TurnContext<'a> {
    text: &'a str,
    chat: bool,
    token: Arc<TurnToken>,
    deadline: Instant,
    caps: ModelCapabilities,
    max_rounds: usize,
    runtime: Option<Arc<dyn AgentRuntimeApi>>,
    volatile: String,
    out: &'a Sender<Result<String, TurnError>>,
    cache: HashMap<(String, String), String>,
}

/// Resultado de una respuesta del modelo.
struct RoundResult {
    tool_calls: Vec<Value>,
    complete: Vec<bool>,
    finish_reason: Option<String>,
    first_chunk_ms: Option<f64>,
    tripped: bool,
}

/// Cómo sigue el bucle tras una vuelta.
enum RoundFlow {
    Continue,
    Break,
    Done,
}

fn emit(out: &Sender<Result<String, TurnError>>, text: &str) -> Result<(), TurnError> {
    out.send(Ok(text.to_string()))
        .map_err(|_| TurnError::cancelled("turno cancelado por el usuario"))
}

impl Messenger {
    fn system_content(&self) -> String {
        let history = self.history();
        let content = history
            .messages
            .first()
            .and_then(|message| message.get("content"));
        text_or_empty(content)
    }

    fn set_system(&self, content: &str) {
        let mut history = self.history();
        let message = json!({"role": "system", "content": content});
        if history.messages.is_empty() {
            history.messages.push(message);
        } else {
            history.messages[0] = message;
        }
    }

    /// Dueño del historial desde aquí y notas de agentes pendientes.
    fn begin_turn_history(&self, text: &str) {
        {
            let mut history = self.history();
            history.last_user_text = text.to_string();
            history.turn_active = true;
        }
        let pending = self.take_pending_notes();
        if !pending.is_empty() {
            self.history().messages.extend(pending);
        }
    }

    /// `runtime.begin_turn` + prompt; si falla, el turno sigue sin herramientas.
    fn start_runtime_prompt(&self, text: &str) -> (Option<Arc<dyn AgentRuntimeApi>>, String) {
        let mut runtime = self.runtime();
        let mut volatile = String::new();
        if let Some(active) = runtime.clone() {
            match active.begin_turn(text) {
                Ok(()) => {
                    let (stable, extra) = self.prompt_parts(&active);
                    volatile = extra;
                    self.set_system(&stable);
                }
                Err(error) => {
                    tracing::warn!(
                        "Núcleo agéntico: {error}; sigo sin herramientas en este turno."
                    );
                    runtime = None;
                }
            }
        }
        if runtime.is_none() {
            // Sin núcleo agéntico el prompt es fijo: se rehace para no
            // arrastrar el del chat.
            let prompt = self.system_prompt();
            self.set_system(&prompt);
        }
        (runtime, volatile)
    }

    /// Deja el historial válido y añade el mensaje del usuario; devuelve el prompt base.
    fn normalize_history(&self, text: &str) -> String {
        let core = self.system_content();
        let mut history = self.history();
        // Nunca se manda una llamada abierta ni JSON roto de un turno anterior.
        sanitize_history(&mut history.messages);
        repair_structure(&mut history.messages);
        history.messages[0] = json!({"role": "system", "content": core});
        // El historial guarda solo el texto; lo volátil se añade a la petición.
        history
            .messages
            .push(json!({"role": "user", "content": text}));
        core
    }

    /// Compacta el contexto y deja el prompt del turno con el resumen y el modo chat.
    fn prepare_context(
        &self,
        runtime: Option<&Arc<dyn AgentRuntimeApi>>,
        text: &str,
        chat: bool,
        core: &str,
        token: &Arc<TurnToken>,
        volatile: &str,
    ) -> Result<(ModelCapabilities, usize, Instant), TurnError> {
        *lock(&self.shared.last_rounds) = Vec::new();
        let settings = self.settings();
        let caps = self.caps(&settings);
        let max_rounds = settings.tool_round_limit();
        let deadline = Instant::now() + Duration::from_secs_f64(settings.turn_timeout());
        let first_tools = self.tools_for(runtime, text, caps);
        self.manage_context(
            &first_tools,
            self.max_tokens(&settings, chat, !first_tools.is_empty()),
            token,
            volatile,
        )?;
        let summary = self.summary();
        let base = if summary.is_empty() {
            core.to_string()
        } else {
            context::with_summary(core, &summary)
        };
        let base = if chat {
            format!("{base}\n\n{CHAT_MODE_PROMPT}")
        } else {
            base
        };
        self.set_system(&base);
        Ok((caps, max_rounds, deadline))
    }

    /// `_ask_stream`: prepara el contexto y corre las vueltas del turno.
    pub(super) fn drive_turn(
        &self,
        text: &str,
        mode: AskMode,
        token: &Arc<TurnToken>,
        out: &Sender<Result<String, TurnError>>,
    ) -> Result<(), TurnError> {
        let chat = mode == AskMode::Chat;
        self.begin_turn_history(text);
        let (runtime, volatile) = self.start_runtime_prompt(text);
        self.check_cancel(token)?;
        let core = self.normalize_history(text);
        let (caps, max_rounds, deadline) =
            self.prepare_context(runtime.as_ref(), text, chat, &core, token, &volatile)?;
        let mut state = TurnState::new(chat);
        let mut ctx = TurnContext {
            text,
            chat,
            token: token.clone(),
            deadline,
            caps,
            max_rounds,
            runtime,
            volatile,
            out,
            cache: HashMap::new(),
        };
        let outcome = self.round_loop(&mut ctx, &mut state);
        if !state.completed {
            self.close_interrupted(&state.partial, &state.open_calls, state.assistant_appended);
        }
        outcome
    }

    fn round_loop(
        &self,
        ctx: &mut TurnContext<'_>,
        state: &mut TurnState,
    ) -> Result<(), TurnError> {
        for round_index in 0..=ctx.max_rounds {
            self.check_cancel(&ctx.token)?;
            self.check_deadline(ctx)?;
            let remaining = remaining_tools(ctx.runtime.as_ref());
            let final_round =
                state.finish_turn || round_index == ctx.max_rounds || remaining == Some(0);
            let tools = if final_round {
                Vec::new()
            } else {
                self.tools_for(ctx.runtime.as_ref(), ctx.text, ctx.caps)
            };
            let request = self.request(
                ctx.text,
                ctx.chat,
                &tools,
                remaining,
                final_round,
                ctx.caps,
                &ctx.volatile,
            );
            state.partial.clear();
            state.assistant_appended = false;
            let round = self.run_round(&request, ctx, &mut state.partial, &mut state.spoke)?;
            let round_note = self.round_note(round.first_chunk_ms);
            let flow = if round.tool_calls.is_empty() {
                self.finish_text_round(ctx, state, round, round_note, final_round)?
            } else {
                self.run_tool_round(ctx, state, round, round_note, final_round)?
            };
            match flow {
                RoundFlow::Done => return Ok(()),
                RoundFlow::Break => break,
                RoundFlow::Continue => {}
            }
        }
        let fallback = "Conservé los resultados obtenidos, pero no pude completar la tarea. \
Todavía no hay una entrega final confirmada.";
        self.record_answer(fallback, ctx.runtime.as_ref());
        state.completed = true;
        emit(ctx.out, fallback)?;
        Ok(())
    }

    fn check_deadline(&self, ctx: &TurnContext<'_>) -> Result<(), TurnError> {
        if Instant::now() > ctx.deadline {
            return Err(TurnError::Timeout(TURN_TIMEOUT_TEXT.to_string()));
        }
        Ok(())
    }

    /// Anuncia y ejecuta las herramientas de una vuelta con llamadas.
    fn run_tool_round(
        &self,
        ctx: &mut TurnContext<'_>,
        state: &mut TurnState,
        round: RoundResult,
        mut round_note: String,
        final_round: bool,
    ) -> Result<RoundFlow, TurnError> {
        if !state.partial.is_empty() {
            // Separa el texto del modelo de lo que siga.
            emit(ctx.out, " ")?;
        }
        if !state.spoke {
            // Los argumentos ya están completos: el aviso puede decir qué se
            // hace (y callar si es rápido).
            if let Some(ack) = round
                .tool_calls
                .iter()
                .find_map(|call| tool_ack(call_name(call), call_arguments(call)))
            {
                state.spoke = true;
                emit(ctx.out, &format!("{ack} "))?;
            }
        }
        let content = state.partial.join("");
        self.history().messages.push(json!({
            "role": "assistant",
            "content": content,
            "tool_calls": &round.tool_calls,
        }));
        state.assistant_appended = true;
        state.open_calls = round.tool_calls.clone();
        state.partial.clear();
        for (number, call) in round.tool_calls.iter().enumerate() {
            if ctx.token.is_cancelled() {
                break;
            }
            let complete = round.complete.get(number).copied().unwrap_or(false);
            let timing = self.run_one_tool(ctx, state, call, complete, final_round)?;
            round_note.push_str(&timing);
        }
        lock(&self.shared.last_rounds).push(round_note);
        if ctx.token.is_cancelled() {
            return Err(TurnError::cancelled(
                "turno cancelado durante una herramienta",
            ));
        }
        state.open_calls.clear();
        Ok(if final_round {
            RoundFlow::Break
        } else {
            RoundFlow::Continue
        })
    }

    fn run_one_tool(
        &self,
        ctx: &mut TurnContext<'_>,
        state: &mut TurnState,
        call: &Value,
        complete: bool,
        final_round: bool,
    ) -> Result<String, TurnError> {
        let name = call_name(call).to_string();
        let tool_began = Instant::now();
        let content = match self.run_tool(
            ctx.runtime.as_ref(),
            call,
            complete,
            &mut ctx.cache,
            !final_round,
            &ctx.token,
        ) {
            Ok((content, repeated)) => {
                state.finish_turn = state.finish_turn || repeated;
                content
            }
            Err(TurnError::Cancelled(message)) => {
                return Err(TurnError::Cancelled(message));
            }
            Err(error) => {
                tracing::warn!("Herramienta {name}: {error}");
                // El aviso sale por voz, no solo al log.
                emit(
                    ctx.out,
                    "No pude ejecutar la herramienta: el núcleo agéntico falló.",
                )?;
                let message = match &error {
                    TurnError::Runtime(failure) => format!(
                        "Error del núcleo agéntico: {}: {}",
                        failure.kind, failure.message
                    ),
                    other => format!("Error del núcleo agéntico: RuntimeError: {other}"),
                };
                ez_core::pyjson::dumps_with(
                    &json!({"ok": false, "error": message}),
                    &ez_core::pyjson::DumpOptions::utf8(),
                )
            }
        };
        self.history().messages.push(json!({
            "role": "tool",
            "tool_call_id": call["id"].clone(),
            "content": content,
        }));
        Ok(format!(
            ", {name} {:.0} ms",
            tool_began.elapsed().as_secs_f64() * 1000.0
        ))
    }

    /// Texto final de una vuelta: marcado, aviso de longitud y respuesta.
    fn finish_text_round(
        &self,
        ctx: &TurnContext<'_>,
        state: &mut TurnState,
        round: RoundResult,
        round_note: String,
        final_round: bool,
    ) -> Result<RoundFlow, TurnError> {
        lock(&self.shared.last_rounds).push(round_note);
        if round.tripped {
            // «Llamó» a una herramienta solo en texto: no se ejecutó nada.
            tracing::warn!("El modelo escribió marcado de herramienta como texto.");
            state.partial.push(TOOL_MARKUP_NOTICE.to_string());
            emit(ctx.out, TOOL_MARKUP_NOTICE)?;
        }
        let mut content = py_strip(&state.partial.join("")).to_string();
        if round.finish_reason.as_deref() == Some("length") && !content.is_empty() {
            content.push_str(LENGTH_NOTICE);
            emit(ctx.out, LENGTH_NOTICE)?;
        }
        if final_round && content.is_empty() {
            return Ok(RoundFlow::Break);
        }
        if content.is_empty() {
            // Ni texto ni herramienta: la voz no puede quedarse muda.
            emit(ctx.out, EMPTY_ANSWER)?;
            self.record_answer(EMPTY_ANSWER, ctx.runtime.as_ref());
            state.completed = true;
            return Ok(RoundFlow::Done);
        }
        self.record_answer(&content, ctx.runtime.as_ref());
        state.assistant_appended = true;
        state.completed = true;
        Ok(RoundFlow::Done)
    }

    fn round_note(&self, first_chunk_ms: Option<f64>) -> String {
        let round = lock(&self.shared.last_rounds).len() + 1;
        let timing = match first_chunk_ms {
            Some(ms) => format!("{ms:.0} ms al 1.er fragmento"),
            None => "sin fragmentos".to_string(),
        };
        format!("llamada {round}: {timing}")
    }

    /// Una respuesta del modelo: lee el flujo, filtra el marcado y sella las llamadas.
    fn run_round(
        &self,
        request: &Value,
        ctx: &TurnContext<'_>,
        partial: &mut Vec<String>,
        spoke: &mut bool,
    ) -> Result<RoundResult, TurnError> {
        let began = Instant::now();
        let mut first_chunk: Option<f64> = None;
        let mut finish_reason: Option<String> = None;
        let mut calls: BTreeMap<usize, Value> = BTreeMap::new();
        let mut guard = ToolMarkupGuard::new();
        let mut request = request.clone();
        let stream = self.open_stream(&mut request, &ctx.token, Some(ctx.deadline))?;
        let active = Arc::new(ActiveStream {
            token: ctx.token.clone(),
            stream: std::sync::Mutex::new(stream),
        });
        *lock(&self.shared.active_stream) = Some(active.clone());
        let outcome = self.consume_stream(
            &active,
            ctx,
            partial,
            spoke,
            &mut guard,
            &mut finish_reason,
            &mut calls,
            began,
            &mut first_chunk,
        );
        {
            let mut slot = lock(&self.shared.active_stream);
            if slot
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &active))
            {
                *slot = None;
            }
        }
        lock(&active.stream).close();
        outcome?;
        let tail = guard.flush();
        if !tail.is_empty() {
            partial.push(tail.clone());
            emit(ctx.out, &tail)?;
        }
        let mut ordered: Vec<Value> = calls.into_values().collect();
        let complete = seal_tool_calls(&mut ordered, finish_reason.as_deref());
        Ok(RoundResult {
            tool_calls: ordered,
            complete,
            finish_reason,
            first_chunk_ms: first_chunk,
            tripped: guard.tripped,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn consume_stream(
        &self,
        active: &Arc<ActiveStream>,
        ctx: &TurnContext<'_>,
        partial: &mut Vec<String>,
        spoke: &mut bool,
        guard: &mut ToolMarkupGuard,
        finish_reason: &mut Option<String>,
        calls: &mut BTreeMap<usize, Value>,
        began: Instant,
        first_chunk: &mut Option<f64>,
    ) -> Result<(), TurnError> {
        loop {
            self.check_cancel(&ctx.token)?;
            self.check_deadline(ctx)?;
            let next = lock(&active.stream).next();
            let Some(chunk) = next else {
                return Ok(());
            };
            let chunk: crate::openai::Chunk = chunk.map_err(|error: LlmError| {
                if ctx.token.is_cancelled() {
                    TurnError::cancelled("turno cancelado por el usuario")
                } else {
                    TurnError::Llm(error)
                }
            })?;
            self.check_cancel(&ctx.token)?;
            if let Some(reason) = chunk.finish_reason.filter(|reason| !reason.is_empty()) {
                *finish_reason = Some(reason);
            }
            if first_chunk.is_none() && (chunk.content.is_some() || !chunk.tool_calls.is_empty()) {
                *first_chunk = Some(began.elapsed().as_secs_f64() * 1000.0);
            }
            if let Some(content) = chunk.content {
                let safe = guard.feed(&content);
                if !safe.is_empty() {
                    *spoke = *spoke || !py_strip(&safe).is_empty();
                    partial.push(safe.clone());
                    emit(ctx.out, &safe)?;
                }
            }
            for delta in chunk.tool_calls {
                let item = calls.entry(delta.index).or_insert_with(|| {
                    json!({
                        "id": "",
                        "type": "function",
                        "function": {"name": "", "arguments": ""},
                    })
                });
                if let Some(id) = delta.id.filter(|id| !id.is_empty()) {
                    item["id"] = Value::String(id);
                }
                if let Some(name) = delta.name.filter(|name| !name.is_empty()) {
                    append_text(&mut item["function"]["name"], &name);
                }
                if !delta.arguments.is_empty() {
                    append_text(&mut item["function"]["arguments"], &delta.arguments);
                }
            }
        }
    }
}
