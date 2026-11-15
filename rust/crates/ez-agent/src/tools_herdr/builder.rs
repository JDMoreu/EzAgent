//! Port of `src/ezagent/agent/tools_herdr.py` (F4-12).
//!
//! Construcción de las dos herramientas de Herdr con estado compartido:
//! `delegate_to_agent` (nivel estático 2, siempre con resumen por código) y
//! `voice_control` (nivel 1).

use std::sync::Arc;

use ez_herdr::herdr::Herdr;
use serde_json::Map;
use serde_json::Value;

use super::args::{delegate_parameters, shared_state, voice_parameters, HerdrCallbacks};
use super::delegate::handle_delegate;
use super::port::HerdrPort;
use super::summaries::{delegate_info, voice_info, with_labels};
use super::voice::handle_voice;
use crate::registry::ToolSpec;

const DELEGATE_DESCRIPTION: &str = "Delega en un agente de Herdr o consulta su estado. list enumera los agentes vivos; get/read/focus/watch operan sobre una dirección; send envía una tarea a un agente; run abre una pestaña visible y lanza un agente (con tarea o sin ella, con model y effort; then encadena más agentes); open_terminal abre una terminal vacía; agents muestra el catálogo; configure cambia modelo/esfuerzo de un agente abierto; kill termina un agente. Por defecto todo debe verse en Herdr: usa background=true solo si el usuario lo pide. Usa gui=true para tareas de interfaz gráfica y notify_on_done o close_listening para avisar al terminar o cerrar la escucha.";

const VOICE_DESCRIPTION: &str = "Controla la escucha y los avisos de voz. close cierra la escucha al terminar de hablar; close_and_watch la cierra y avisa cuando termine el agente; mic_mute silencia o reactiva el micrófono; repeat_last repite el último resumen; mute silencia los avisos durante N minutos. Úsalo cuando el usuario pida cerrar, silenciarse o repetir.";

/// Construye las dos herramientas de Herdr con estado compartido.
pub fn build_herdr_tools(herdr: Arc<Herdr>, callbacks: HerdrCallbacks) -> Vec<ToolSpec> {
    build_herdr_tools_with_port(Arc::new(herdr), callbacks)
}

/// Igual que [`build_herdr_tools`], con un adaptador inyectado para las pruebas.
pub fn build_herdr_tools_with_port(
    herdr: Arc<dyn HerdrPort>,
    callbacks: HerdrCallbacks,
) -> Vec<ToolSpec> {
    let state = shared_state();

    let run_herdr = Arc::clone(&herdr);
    let run_callbacks = callbacks.clone();
    let run_state = Arc::clone(&state);
    let mut delegate = ToolSpec::new(
        "delegate_to_agent",
        DELEGATE_DESCRIPTION,
        delegate_parameters(),
        // DECISION: la interfaz describe_fn fija el nivel real de cada acción.
        // El nivel estático es el máximo de la herramienta para que cualquier
        // consumidor que no use describe() sea restrictivo, nunca permisivo.
        2,
        Arc::new(move |args: &Map<String, Value>| {
            let result = handle_delegate(run_herdr.as_ref(), &run_callbacks, &run_state, args);
            let label_for = |address: &str| run_herdr.label_for(address);
            Ok(with_labels(&label_for, result).into())
        }),
    );
    // DECISION: ToolSpec solo permite un booleano para toda la herramienta.
    // «read» debe marcar taint; marcar también list/get es lo conservador y no
    // pierde la garantía de la sección 5.4.
    delegate.untrusted_output = true;
    // DECISION: run abre la terminal (hasta 45 s), espera a que arranque el
    // agente (hasta 30 s) y escribe modelo, esfuerzo y tarea; 120 s evita que
    // el registro corte un lanzamiento válido.
    delegate.timeout_s = 120.0;
    let describe_herdr = Arc::clone(&herdr);
    delegate.describe_fn = Some(Arc::new(move |args| {
        delegate_info(args, &|address| describe_herdr.label_for(address))
    }));

    let voice_herdr = Arc::clone(&herdr);
    let mut voice = ToolSpec::new(
        "voice_control",
        VOICE_DESCRIPTION,
        voice_parameters(),
        1,
        Arc::new(move |args: &Map<String, Value>| {
            let result = handle_voice(&callbacks, &state, args);
            let label_for = |address: &str| voice_herdr.label_for(address);
            Ok(with_labels(&label_for, result).into())
        }),
    );
    voice.untrusted_output = false;
    voice.timeout_s = 10.0;
    let describe_voice = Arc::clone(&herdr);
    voice.describe_fn = Some(Arc::new(move |args| {
        voice_info(args, &|address| describe_voice.label_for(address))
    }));

    vec![delegate, voice]
}

#[cfg(test)]
mod tests;
