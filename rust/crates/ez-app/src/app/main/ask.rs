//! `--say` y `--ask`: Kokoro y un único turno sin micrófono.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use ez_agent::runtime::{AgentRuntime, HerdrApi, RuntimeCallbacks as AgentCallbacks};
use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use ez_llm::messenger::{AgentRuntimeApi, Messenger, MessengerCallbacks};
use serde_json::Value;

use super::super::core::log;

/// `Herdr` visto por el núcleo agéntico desde la línea de comandos.
struct CliHerdr(Arc<Herdr>);

impl HerdrApi for CliHerdr {
    fn catalog_text(&self) -> String {
        ez_herdr::driver::AgentDriver::new(Arc::clone(&self.0), self.0.config()).catalog_text()
    }

    fn context_text(&self) -> String {
        self.0.context_text()
    }

    fn list_agents(&self) -> Vec<Value> {
        self.0
            .list_agents(20.0)
            .map(|agents| agents.into_iter().map(Value::Object).collect())
            .unwrap_or_default()
    }
}

/// `Messenger(...)` de `main --ask`, con el runtime de herramientas de Herdr.
fn cli_messenger(cfg: VoiceConfig) -> Messenger {
    let herdr = Arc::new(Herdr::new(cfg.clone()));
    let runtime = AgentRuntime::new(
        cfg.clone(),
        Some(Arc::new(CliHerdr(herdr)) as Arc<dyn HerdrApi>),
        AgentCallbacks::default(),
        None,
    )
    .ok()
    .map(|runtime| Arc::new(runtime) as Arc<dyn AgentRuntimeApi>);
    Messenger::new(cfg, MessengerCallbacks::default(), runtime)
}

pub(super) fn run_say(cfg: &VoiceConfig, text: &str) -> i32 {
    match ez_tts::speaker::Speaker::new(cfg.clone()) {
        Ok(speaker) => {
            let stop = AtomicBool::new(false);
            let result = speaker.speak(text, &stop, None, None);
            speaker.close();
            match result {
                Ok(_) => 0,
                Err(error) => {
                    log(format!("Kokoro: {error}"));
                    1
                }
            }
        }
        Err(error) => {
            log(format!("No pude cargar Kokoro: {error}"));
            1
        }
    }
}

pub(super) fn run_ask(cfg: &VoiceConfig, text: &str) -> i32 {
    let messenger = cli_messenger(cfg.clone());
    match messenger.ask(text) {
        Ok(answer) => {
            super::output::print_line(&answer);
            0
        }
        Err(error) => {
            log(format!("No pude responder: {error}"));
            1
        }
    }
}
