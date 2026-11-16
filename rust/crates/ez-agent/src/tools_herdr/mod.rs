//! Port of `src/ezagent/agent/tools_herdr.py` (F4-12).
//!
//! Herramientas del núcleo agéntico para delegar en Herdr y controlar la voz.
//! Expone dos herramientas con `action` para no inflar el prompt del LLM:
//! `delegate_to_agent` (consulta, delegación y vigilancia) y `voice_control`
//! (escucha, micrófono y avisos). Los resúmenes de confirmación los genera el
//! código, nunca el LLM. Las llamadas a Herdr pueden tardar decenas de
//! segundos; este módulo no abre ventanas ni lanza procesos por su cuenta.

pub mod args;
pub mod builder;
pub mod delegate;
pub mod names;
pub mod port;
pub mod summaries;
pub mod voice;

pub use args::HerdrCallbacks;
pub use builder::build_herdr_tools;
pub use names::Names;
pub use port::HerdrPort;
