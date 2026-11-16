//! Commands of the swarm service (`service.py`, commands section).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::db::DbResult;
use crate::models::{
    task_id, Member, SwarmError, Task, BROADCAST, DEFAULT_LEASE_SECS, MAX_INBOX, MAX_ROUNDS,
};
use crate::protocol;

use super::{Call, SwarmService, BURST_MESSAGES, BURST_SECS, MAX_TASKS, MAX_WAIT_SECS};

impl SwarmService {
    pub(super) fn next(&self, call: &Call) -> DbResult<String> {
        let mut lines = Vec::new();
        if self.store.mark_briefed(&call.swarm, &call.me.name)? {
            lines.push(self.briefing(&call.swarm, &call.me.name)?);
            lines.push(String::new());
        }
        let messages = self
            .store
            .inbox(&call.swarm, &call.me.name, true, MAX_INBOX as i64)?;
        if !messages.is_empty() {
            lines.push(format!("Mensajes ({}):", messages.len()));
            lines.extend(messages.iter().map(|message| message.line()));
        }
        let mine = self.store.mine(&call.swarm, &call.me.name)?;
        let every = call.options.contains_key("all");
        let taken = if every || mine.is_empty() {
            self.store.claim(&call.swarm, &call.me.name, None, every)?
        } else {
            Vec::new()
        };
        let tasks = self.store.tasks(&call.swarm)?;
        for task in &mine {
            lines.push(format!(
                "En curso: {}",
                describe_task(task, &tasks, &call.me)
            ));
        }
        for task in &taken {
            lines.push(format!(
                "Tu tarea: {}",
                describe_task(task, &tasks, &call.me)
            ));
        }
        if mine.is_empty() && taken.is_empty() {
            lines.push(
                if self.finished(&call.swarm)? {
                    protocol::ALL_DONE
                } else {
                    protocol::IDLE
                }
                .to_string(),
            );
        }
        Ok(lines.join("\n"))
    }

    pub(super) fn finished(&self, swarm: &str) -> DbResult<bool> {
        Ok(self.status(swarm)?.state == "done")
    }

    /// Current task the command refers to, plus the remaining text.
    fn own_task(&self, call: &Call) -> DbResult<(i64, String)> {
        let mine: Vec<i64> = self
            .store
            .mine(&call.swarm, &call.me.name)?
            .iter()
            .map(|task| task.id)
            .collect();
        let first = call.words.first().map_or("", String::as_str);
        let explicit = is_task_word(first)
            || (is_digits(first) && task_id(first).is_ok_and(|id| mine.contains(&id)));
        if explicit {
            return Ok((task_id(first)?, call.text(1)));
        }
        if mine.len() == 1 {
            return Ok((mine[0], call.text(0)));
        }
        if mine.is_empty() {
            return Err(SwarmError::new(format!(
                "No tienes tareas en curso. Usa: {} next",
                self.command
            ))
            .into());
        }
        let listed = mine
            .iter()
            .map(|id| format!("T{id}"))
            .collect::<Vec<_>>()
            .join(", ");
        Err(SwarmError::new(format!(
            "Tienes varias en curso ({listed}): indica cuál, p. ej. done T{}.",
            mine[0]
        ))
        .into())
    }

    pub(super) fn done(&self, call: &Call) -> DbResult<String> {
        let (task, result) = self.own_task(call)?;
        if result.is_empty() {
            return Err(SwarmError::new(format!(
                "Falta el resumen: {} done \"qué hiciste en una línea\"",
                self.command
            ))
            .into());
        }
        let reference = call.options.get("ref").map_or("", String::as_str);
        self.store
            .finish(&call.swarm, &call.me.name, task, &result, true, reference)?;
        if self.finished(&call.swarm)? {
            self.store.set_status(&call.swarm, "done")?;
            self.changed(&call.swarm, &call.me.name, "finished");
        }
        Ok(format!("T{task} hecha.\n{}", self.next(call)?))
    }

    pub(super) fn fail(&self, call: &Call) -> DbResult<String> {
        let (task, reason) = self.own_task(call)?;
        if reason.is_empty() {
            return Err(SwarmError::new(format!(
                "Falta el motivo: {} fail \"por qué no puedes seguir\"",
                self.command
            ))
            .into());
        }
        self.store
            .finish(&call.swarm, &call.me.name, task, &reason, false, "")?;
        Ok(format!(
            "T{task} marcada como fallida; se avisó al equipo y al usuario. Termina tu turno."
        ))
    }

    pub(super) fn fix(&self, call: &Call) -> DbResult<String> {
        if call.words.len() < 2 {
            return Err(SwarmError::new(format!(
                "Uso: {} fix <T> \"qué falta\" [--ref informe]",
                self.command
            ))
            .into());
        }
        let target = task_id(&call.words[0])?;
        let before: HashSet<i64> = self
            .store
            .mine(&call.swarm, &call.me.name)?
            .iter()
            .map(|task| task.id)
            .collect();
        let reference = call.options.get("ref").map_or("", String::as_str);
        let (task, exhausted) =
            self.store
                .reopen(&call.swarm, target, &call.me.name, &call.text(1), reference)?;
        if exhausted {
            return Ok(format!(
                "T{target} superó {MAX_ROUNDS} rondas de corrección; se avisó al usuario. \
                 Termina tu turno."
            ));
        }
        let after: HashSet<i64> = self
            .store
            .mine(&call.swarm, &call.me.name)?
            .iter()
            .map(|task| task.id)
            .collect();
        let mut released: Vec<i64> = before.difference(&after).copied().collect();
        released.sort_unstable();
        let mut text = format!("T{target} reabierta (ronda {}/{MAX_ROUNDS}).", task.rounds);
        if !released.is_empty() {
            let names = released
                .iter()
                .map(|id| format!("T{id}"))
                .collect::<Vec<_>>()
                .join(", ");
            text.push_str(&format!(
                " {names} volverá a ti cuando esté corregida. Termina tu turno."
            ));
        }
        Ok(text)
    }

    pub(super) fn add(&self, call: &Call) -> DbResult<String> {
        if self.store.tasks(&call.swarm)?.len() >= MAX_TASKS {
            return Err(SwarmError::new(format!(
                "El enjambre ya tiene {MAX_TASKS} tareas; agrupa el trabajo."
            ))
            .into());
        }
        let options = &call.options;
        let deps: Vec<i64> = match options.get("after") {
            Some(after) => after
                .split(',')
                .filter(|item| !item.trim().is_empty())
                .map(task_id)
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };
        let owner = options
            .get("to")
            .map(|to| to.to_lowercase())
            .unwrap_or_default();
        let parent = match options.get("in").filter(|value| !value.is_empty()) {
            Some(value) => task_id(value)?,
            None => 0,
        };
        let task = self.store.add_task(
            &call.swarm,
            &call.text(0),
            options.get("role").map_or("", String::as_str),
            &owner,
            &deps,
            parent,
        )?;
        Ok(format!("Creada: {}", task.line("")))
    }

    pub(super) fn say(&self, call: &Call, kind: &str) -> DbResult<String> {
        if call.words.len() < 2 {
            let name = if kind == "info" { "say" } else { kind };
            return Err(SwarmError::new(format!(
                "Uso: {} {name} <nombre|all> \"texto\"",
                self.command
            ))
            .into());
        }
        let now = (self.clock)();
        let recent = self
            .store
            .history(&call.swarm, 0, 40)?
            .iter()
            .filter(|message| message.sender == call.me.name && now - message.ts < BURST_SECS)
            .count();
        if recent >= BURST_MESSAGES {
            return Err(SwarmError::new(
                "Demasiados mensajes seguidos. Junta lo que falta en uno solo.",
            )
            .into());
        }
        let target = call.words[0].to_lowercase();
        let to = if ["all", "todos", BROADCAST].contains(&target.as_str()) {
            BROADCAST.to_string()
        } else {
            target
        };
        let reference = call.options.get("ref").map_or("", String::as_str);
        let (message, cut) = self.store.post(
            &call.swarm,
            &call.me.name,
            &call.text(1),
            &to,
            kind,
            reference,
        )?;
        let note = if cut {
            " Se recortó a 280 caracteres: para el detalle usa --ref <archivo>."
        } else {
            ""
        };
        let shown = if to == BROADCAST { "todos" } else { &to };
        Ok(format!("#{} enviado a {shown}.{note}", message.id))
    }

    pub(super) fn inbox(&self, call: &Call) -> DbResult<String> {
        let messages = self
            .store
            .inbox(&call.swarm, &call.me.name, true, MAX_INBOX as i64)?;
        let lines: Vec<String> = messages.iter().map(|message| message.line()).collect();
        Ok(if lines.is_empty() {
            "Sin mensajes nuevos.".to_string()
        } else {
            lines.join("\n")
        })
    }

    pub(super) fn lock_paths(&self, call: &Call) -> DbResult<String> {
        let minutes = call.options.get("min").map_or("", String::as_str);
        let ttl = if is_digits(minutes) {
            minutes
                .parse::<f64>()
                .map_or(DEFAULT_LEASE_SECS, |value| value * 60.0)
        } else {
            DEFAULT_LEASE_SECS
        };
        let conflicts = self
            .store
            .reserve(&call.swarm, &call.me.name, &call.words, ttl)?;
        if !conflicts.is_empty() {
            let taken = conflicts
                .iter()
                .map(|lease| format!("{} lo tiene {}", lease.path, lease.member))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(SwarmError::new(format!(
                "Ocupado: {taken}. Elige otros archivos o coordínalo con say."
            ))
            .into());
        }
        Ok(format!("Reservado: {}.", call.words.join(", ")))
    }

    pub(super) fn unlock(&self, call: &Call) -> DbResult<String> {
        let paths = if call.words.is_empty() {
            None
        } else {
            Some(call.words.as_slice())
        };
        let count = self.store.release(&call.swarm, &call.me.name, paths)?;
        Ok(format!("Reservas liberadas: {count}."))
    }

    pub(super) fn wait(&self, call: &Call) -> DbResult<String> {
        let first = call.words.first().map_or("", String::as_str);
        let seconds = if is_digits(first) {
            first.parse::<f64>().unwrap_or(60.0)
        } else {
            60.0
        }
        .clamp(1.0, MAX_WAIT_SECS);
        let deadline = Instant::now() + Duration::from_secs_f64(seconds);
        let mut closed = self.news.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if *closed {
                break;
            }
            let (unread, ready) = self.pending(&call.swarm, &call.me.name)?;
            if unread > 0 || ready > 0 {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(format!("Sin novedades en {} s.", seconds as i64));
            }
            let timeout = remaining.min(Duration::from_secs_f64(5.0));
            let (guard, _) = self
                .news_cv
                .wait_timeout(closed, timeout)
                .unwrap_or_else(|error| error.into_inner());
            closed = guard;
        }
        drop(closed);
        self.next(call)
    }
}

fn describe_task(task: &Task, tasks: &[Task], me: &Member) -> String {
    let mut text = format!("T{} {}", task.id, task.title);
    if task.rounds != 0 && !task.result.is_empty() {
        text.push_str(&format!(
            " · corregir ({}/{MAX_ROUNDS}): {}",
            task.rounds, task.result
        ));
    }
    // DECISION: el tablero no tiene columna de cuerpo; la nota de una tarea del
    // modo autónomo (la ruta de `correccion-N.md`) viaja en `result` y se le
    // muestra a quien la toma, porque solo se fija antes de empezar.
    if task.rounds == 0 && !task.result.is_empty() {
        text.push_str(&format!(" · {}", task.result));
    }
    for after in tasks
        .iter()
        .filter(|after| after.deps.contains(&task.id))
        .take(2)
    {
        let mine = after.owner == me.name || (after.owner.is_empty() && after.role == me.role);
        if mine {
            text.push_str(&format!(" · luego: T{} {}", after.id, after.title));
        } else {
            let who = if after.owner.is_empty() {
                protocol::role_label(&after.role).to_string()
            } else {
                after.owner.clone()
            };
            text.push_str(&format!(
                " · después sigue {who} con T{} (no la hagas tú)",
                after.id
            ));
        }
    }
    text
}

fn is_digits(text: &str) -> bool {
    // DECISION: Python's `str.isdigit()` accepts every Unicode decimal digit;
    // command arguments are ASCII in practice.
    !text.is_empty() && text.chars().all(|character| character.is_ascii_digit())
}

fn is_task_word(text: &str) -> bool {
    text.len() > 1
        && (text.starts_with('T') || text.starts_with('t'))
        && text[1..]
            .chars()
            .all(|character| character.is_ascii_digit())
}
