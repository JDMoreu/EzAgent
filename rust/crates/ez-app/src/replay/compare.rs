//! Comparación de trazas por hilo (`replay_parity` de F7-01a).
//!
//! El orden entre eventos de hilos distintos no es determinista en ninguna de
//! las dos versiones: se compara **el orden causal de cada etiqueta de hilo por
//! separado**. Dentro de un hilo se ignoran los eventos `driver.step`, porque
//! son del propio arnés y su entrelazado con los de la app varía (lo mostró el
//! Python). `seq` también se ignora: al quitar pasos deja de ser comparable.

use serde_json::{Map, Value};

/// Primera diferencia entre dos trazas.
#[derive(Debug, PartialEq, Eq)]
pub struct Diff {
    /// Etiqueta del hilo (`main#0`, `ezagent-answer#1`…).
    pub thread: String,
    /// Índice dentro de la secuencia del hilo (sin `driver.step`).
    pub index: usize,
    /// Línea esperada (dorada).
    pub expected: String,
    /// Línea producida (o `<fin>`).
    pub produced: String,
}

/// Evento comparable: sin `seq` y sin los pasos del arnés.
type Event = Map<String, Value>;

/// Compara dos trazas en texto; `None` si son equivalentes.
pub fn causal_diff(expected: &str, produced: &str) -> Result<Option<Diff>, String> {
    let expected = group(expected)?;
    let produced = group(produced)?;
    for (thread, wanted) in &expected {
        let got = produced
            .iter()
            .find(|(name, _)| name == thread)
            .map_or(&[][..], |(_, events)| events.as_slice());
        let mut index = 0;
        loop {
            match (wanted.get(index), got.get(index)) {
                (None, None) => break,
                (Some(want), Some(actual)) if same(want, actual) => index += 1,
                (want, actual) => {
                    return Ok(Some(Diff {
                        thread: thread.clone(),
                        index,
                        expected: want.map_or_else(|| "<fin>".to_string(), render),
                        produced: actual.map_or_else(|| "<fin>".to_string(), render),
                    }));
                }
            }
        }
    }
    for (thread, got) in &produced {
        if !expected.iter().any(|(name, _)| name == thread) && !got.is_empty() {
            return Ok(Some(Diff {
                thread: thread.clone(),
                index: 0,
                expected: "<fin>".to_string(),
                produced: got.first().map_or_else(String::new, render),
            }));
        }
    }
    Ok(None)
}

/// Agrupa por etiqueta de hilo, conservando el orden causal.
fn group(text: &str) -> Result<Vec<(String, Vec<Event>)>, String> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: Vec<(String, Vec<Event>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let item: Value = serde_json::from_str(line)
            .map_err(|error| format!("línea de traza no válida ({error}): {line}"))?;
        let Some(object) = item.as_object() else {
            return Err(format!("evento sin objeto: {line}"));
        };
        let thread = object
            .get("thread")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if object.get("event").and_then(Value::as_str) == Some("driver.step") {
            continue;
        }
        let mut comparable = object.clone();
        comparable.remove("seq");
        let position = match order.iter().position(|known| *known == thread) {
            Some(position) => position,
            None => {
                order.push(thread.clone());
                groups.push((thread.clone(), Vec::new()));
                order.len() - 1
            }
        };
        groups[position].1.push(comparable);
    }
    Ok(groups)
}

/// Igualdad de `event` y todos los campos (sin `seq`).
fn same(expected: &Event, produced: &Event) -> bool {
    expected.len() == produced.len()
        && expected
            .iter()
            .all(|(key, value)| produced.get(key) == Some(value))
}

/// Línea JSON con los separadores del Python.
fn render(event: &Map<String, Value>) -> String {
    ez_core::pyjson::dumps_with(
        &Value::Object(event.clone()),
        &ez_core::pyjson::DumpOptions::utf8(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = concat!(
        "{\"thread\": \"main#0\", \"seq\": 1, \"event\": \"hud.show\", \"state\": \"idle\"}\n",
        "{\"thread\": \"main#0\", \"seq\": 2, \"event\": \"driver.step\", \"index\": 1, \"step\": \"x:1\"}\n",
        "{\"thread\": \"main#0\", \"seq\": 3, \"event\": \"hud.show\", \"state\": \"listening\"}\n",
        "{\"thread\": \"ezagent-call#0\", \"seq\": 1, \"event\": \"vad.speech_start\"}\n",
    );

    #[test]
    fn ignores_driver_steps_and_seq() {
        let produced = concat!(
            "{\"thread\": \"main#0\", \"seq\": 1, \"event\": \"hud.show\", \"state\": \"idle\"}\n",
            "{\"thread\": \"main#0\", \"seq\": 2, \"event\": \"hud.show\", \"state\": \"listening\"}\n",
            "{\"thread\": \"ezagent-call#0\", \"seq\": 1, \"event\": \"vad.speech_start\"}\n",
        );
        assert_eq!(causal_diff(GOLDEN, produced).expect("válidas"), None);
    }

    #[test]
    fn reports_the_first_difference_of_a_thread() {
        let produced = concat!(
            "{\"thread\": \"main#0\", \"seq\": 1, \"event\": \"hud.show\", \"state\": \"idle\"}\n",
            "{\"thread\": \"main#0\", \"seq\": 2, \"event\": \"hud.show\", \"state\": \"thinking\"}\n",
        );
        let diff = causal_diff(GOLDEN, produced)
            .expect("válidas")
            .expect("hay diferencia");
        assert_eq!(diff.thread, "main#0");
        assert_eq!(diff.index, 1);
        assert!(diff.expected.contains("listening"));
        assert!(diff.produced.contains("thinking"));
    }

    #[test]
    fn an_extra_thread_is_a_difference() {
        let produced =
            format!("{GOLDEN}{{\"thread\": \"ezagent-other#0\", \"seq\": 1, \"event\": \"x\"}}\n");
        let diff = causal_diff(GOLDEN, &produced)
            .expect("válidas")
            .expect("hay diferencia");
        assert_eq!(diff.thread, "ezagent-other#0");
    }
}
