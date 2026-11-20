//! Novedades de la pantalla de un hijo (`novedades-subagentes.md` §2–§3).
//!
//! Solo lectura: la pantalla se compara con patrones y nunca se ejecuta ni se
//! copia entera a un aviso. Los patrones de fábrica están en el código; si la
//! persona escribe `novedades.toml` en `%APPDATA%\EzAgent`, ese archivo manda.
//!
//! Formato del archivo (una regla por línea, sin distinguir mayúsculas):
//! ```text
//! # agente = patrón; «*» vale para todos
//! opencode = Free limit reached
//! * = quota exceeded
//! stall_min = 10
//! ```
//!
//! Un límite solo se busca en las últimas [`LIMIT_TAIL_LINES`] líneas **con
//! texto** de la pantalla y solo cuenta con el agente parado y la pantalla
//! repetida en dos vueltas: quien escribe o lee sobre límites de uso no está
//! bloqueado.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use chrono::{Local, NaiveTime, TimeZone};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::permission::{classify, DialogKind};
use crate::subagents::agent_label;

use super::{Child, Record, ScreenMemory, TaskState, Tick, Watcher};

/// Tope de la pantalla que se guarda y se compara (letras).
pub(super) const SCREEN_LIMIT: usize = 4_000;

/// Minutos sin cambio de pantalla para avisar «sin avance».
pub(super) const DEFAULT_STALL_MIN: u64 = 10;

/// Líneas del final de la pantalla que se miran para un límite de uso (§3).
pub(super) const LIMIT_TAIL_LINES: usize = 15;

/// Patrones de fábrica de §3: frases completas, nunca palabras sueltas.
const FACTORY_PATTERNS: [&str; 7] = [
    "Free limit reached",
    "5 hour usage limit reached",
    "You've hit your usage limit",
    "Usage limit reached",
    "rate limit reached",
    "quota exceeded",
    // MiniMax Code avisa así cuando se acaba el crédito (error 50110, 19-nov).
    "Check /usage or switch provider",
];

static USAGE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Una regla: el agente al que aplica (`None` = todos) y el texto buscado.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    agent: Option<String>,
    text: String,
}

/// Los patrones y el umbral de «sin avance» de una vuelta.
#[derive(Debug, Clone)]
pub(super) struct Novelties {
    rules: Vec<Rule>,
    pub(super) stall_min: u64,
}

impl Default for Novelties {
    fn default() -> Self {
        Self::factory(DEFAULT_STALL_MIN)
    }
}

impl Novelties {
    fn factory(stall_min: u64) -> Self {
        Self {
            rules: FACTORY_PATTERNS
                .iter()
                .map(|text| Rule {
                    agent: None,
                    text: (*text).to_string(),
                })
                .collect(),
            stall_min,
        }
    }

    /// Los patrones del archivo o los de fábrica si no hay archivo válido.
    pub(super) fn load(path: Option<&Path>) -> Self {
        let Some(text) = path.and_then(|path| std::fs::read_to_string(path).ok()) else {
            return Self::factory(DEFAULT_STALL_MIN);
        };
        let parsed = Self::parse(&text);
        if parsed.rules.is_empty() {
            // DECISION: un archivo sin patrones (vacío o solo comentarios) no
            // apaga la detección; se usan los de fábrica.
            return Self::factory(parsed.stall_min);
        }
        parsed
    }

    fn parse(text: &str) -> Self {
        let mut rules = Vec::new();
        let mut stall_min = DEFAULT_STALL_MIN;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(value) = line.strip_prefix("stall_min") {
                if let Some(value) = value.trim_start().strip_prefix('=') {
                    if let Ok(minutes) = value.trim().parse::<u64>() {
                        if minutes > 0 {
                            stall_min = minutes;
                        }
                    }
                }
                continue;
            }
            let Some((agent, pattern)) = line.split_once('=') else {
                continue;
            };
            let agent = unquote(agent);
            let pattern = unquote(pattern);
            if pattern.is_empty() {
                continue;
            }
            let agent = if agent.is_empty() || agent == "*" {
                None
            } else {
                Some(agent.to_lowercase())
            };
            rules.push(Rule {
                agent,
                text: pattern.to_string(),
            });
        }
        Self { rules, stall_min }
    }

    /// La primera regla que aparece en el final de la pantalla (§3).
    pub(super) fn limit_hit(&self, agent: &str, screen: &str, now_ms: u64) -> Option<LimitHit> {
        let tail = last_lines(screen, LIMIT_TAIL_LINES);
        let screen = tail.to_lowercase();
        let agent = agent.to_lowercase();
        let matched = self.rules.iter().find(|rule| {
            rule.agent.as_deref().is_none_or(|only| only == agent)
                && screen.contains(&rule.text.to_lowercase())
        })?;
        Some(LimitHit {
            pattern: matched.text.to_lowercase(),
            until_ms: reset_time_ms(&tail, now_ms),
        })
    }
}

/// Un límite de uso detectado: el patrón que coincidió y cuándo se reinicia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LimitHit {
    pattern: String,
    pub(super) until_ms: Option<u64>,
}

impl LimitHit {
    /// Texto corto para el `detail` del registro; nunca copia la pantalla.
    pub(super) fn detail(&self) -> String {
        let cause = if self.pattern.contains("free limit") {
            "límite de uso del plan gratuito"
        } else {
            "límite de uso"
        };
        match self.until_ms.and_then(clock_text) {
            Some(clock) => format!("{cause} (se reinicia a las {clock})"),
            None => cause.to_string(),
        }
    }
}

fn unquote(text: &str) -> &str {
    let text = text.trim();
    for quote in ['"', '\''] {
        if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            return &text[1..text.len() - 1];
        }
    }
    text
}

/// Los últimos `max` caracteres, en un límite de caracteres válido.
pub(super) fn screen_tail(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    text.chars().skip(count - max).collect()
}

/// Las últimas `count` líneas **con texto** de la pantalla, unidas con `\n`.
///
/// El TUI deja muchas filas vacías y de bordes (`┃`, `╹▀▀▀`) debajo de la
/// conversación; no cuentan: una línea con texto tiene al menos una letra o
/// un dígito (§3, caso real de `hc read --lines 0` en OpenCode).
pub(super) fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| line.chars().any(char::is_alphanumeric))
        .collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// La hora de reinicio que mencione la pantalla, en milisegundos desde epoch.
pub(super) fn reset_time_ms(screen: &str, now_ms: u64) -> Option<u64> {
    if let Some(captures) = at_regex().captures(screen) {
        let hour: u32 = captures.get(1)?.as_str().parse().ok()?;
        let minute: u32 = captures
            .get(2)
            .and_then(|value| value.as_str().parse().ok())
            .unwrap_or(0);
        let marker = captures
            .get(3)
            .map(|value| value.as_str().to_ascii_lowercase());
        let hour = match marker.as_deref() {
            Some("p") if hour < 12 => hour + 12,
            Some("a") if hour == 12 => 0,
            _ => hour,
        };
        if hour > 23 || minute > 59 {
            return None;
        }
        return clock_ms(now_ms, hour, minute);
    }
    if let Some(captures) = in_regex().captures(screen) {
        let hours: u64 = captures
            .get(1)
            .and_then(|value| value.as_str().parse().ok())
            .unwrap_or(0);
        let minutes: u64 = captures
            .get(2)
            .and_then(|value| value.as_str().parse().ok())
            .unwrap_or(0);
        let delta = hours
            .saturating_mul(3_600_000)
            .saturating_add(minutes.saturating_mul(60_000));
        if delta > 0 {
            return Some(now_ms.saturating_add(delta));
        }
    }
    None
}

fn at_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\bresets?\s+at\s+(\d{1,2})(?::(\d{2}))?\s*([ap])\.?\s*m\.?")
            .expect("regex de «resets at»")
    })
}

fn in_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\bresets?\s+in\s+(?:(\d+)\s*(?:hours?|hrs?|h))?\s*(?:(\d+)\s*(?:minutes?|mins?|m))?",
        )
        .expect("regex de «resets in»")
    })
}

/// Hoy (o mañana si ya pasó) a esa hora local.
fn clock_ms(now_ms: u64, hour: u32, minute: u32) -> Option<u64> {
    let now = Local
        .timestamp_millis_opt(i64::try_from(now_ms).ok()?)
        .single()?;
    let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
    let mut date = now.date_naive();
    let mut moment = Local.from_local_datetime(&date.and_time(time)).earliest()?;
    if moment.timestamp_millis() <= now.timestamp_millis() {
        date = date.succ_opt()?;
        moment = Local.from_local_datetime(&date.and_time(time)).earliest()?;
    }
    u64::try_from(moment.timestamp_millis()).ok()
}

fn clock_text(ms: u64) -> Option<String> {
    let moment = Local
        .timestamp_millis_opt(i64::try_from(ms).ok()?)
        .single()?;
    Some(moment.format("%H:%M").to_string())
}

impl Watcher {
    /// Detecta límites, diálogos, «sin avance» y la vuelta a trabajar.
    pub(super) fn detect_novelties(
        &mut self,
        records: &[Record],
        children: &[Child],
        tick: &mut Tick,
    ) {
        let novelties = Novelties::load(self.novelty_file.as_deref());
        let now = self.runtime.now_ms();
        for record in records {
            if !matches!(record.state, TaskState::InProgress | TaskState::Blocked) {
                continue;
            }
            let Some(child) = children
                .iter()
                .find(|child| child.address == record.terminal)
            else {
                continue;
            };
            // Una terminal muerta ya la marca `mark_failed`.
            if !child.live {
                continue;
            }
            let Some(screen) = self.read_screen(&record.terminal) else {
                continue;
            };
            match record.state {
                TaskState::InProgress => {
                    self.inspect_running(record, child, &novelties, &screen, now, tick);
                }
                TaskState::Blocked => {
                    self.inspect_blocked(record, child, &novelties, &screen, now, tick);
                }
                _ => {}
            }
        }
    }

    fn inspect_running(
        &mut self,
        record: &Record,
        child: &Child,
        novelties: &Novelties,
        screen: &str,
        now: u64,
        tick: &mut Tick,
    ) {
        let change = self.observe_screen(&record.name, screen, now);
        // §3: un límite solo cuenta con el agente parado y la pantalla quieta
        // desde la vuelta anterior; si está trabajando, la pantalla es dato.
        // La primera lectura no es estable: hace falta verla dos veces iguales.
        if child.agent_state != "working" && change == Some(false) {
            if let Some(hit) = novelties.limit_hit(&record.agent, screen, now) {
                let detail = hit.detail();
                if self.block(record, &detail) {
                    tick.blocked.push(record.name.clone());
                    if let Err(error) = record_usage(
                        self.registry.root(),
                        &record.agent,
                        record.model.as_deref(),
                        hit.until_ms,
                        now,
                    ) {
                        tracing::warn!("EzAgent: no pude anotar uso.json: {error}");
                    }
                    // El reintento automático ya ve el agotamiento en `uso.json`.
                    self.trigger_auto_retry(record, &detail, tick);
                }
                return;
            }
        }
        if child.agent_state == "blocked" || is_dialog(child, screen) {
            if self.block(record, "diálogo o permiso pendiente") {
                tick.blocked.push(record.name.clone());
            }
            return;
        }
        if child.agent_state == "idle" && change != Some(true) {
            self.check_stall(record, novelties.stall_min, now, tick);
        }
    }

    fn inspect_blocked(
        &mut self,
        record: &Record,
        child: &Child,
        novelties: &Novelties,
        screen: &str,
        now: u64,
        tick: &mut Tick,
    ) {
        let hit = novelties.limit_hit(&record.agent, screen, now);
        let changed = self.observe_screen(&record.name, screen, now) == Some(true);
        if child.agent_state != "working" && !(changed && hit.is_none()) {
            return;
        }
        let changed_state = self
            .registry
            .transition(
                &record.parent,
                &record.name,
                TaskState::InProgress,
                "vigilante",
                "siguió trabajando",
                |_| {},
            )
            .is_ok();
        if changed_state {
            tick.resumed.push(record.name.clone());
            if let Some(memory) = self.screens.get_mut(&record.name) {
                memory.changed_at = now;
                memory.stall_notified = false;
            }
        }
    }

    fn block(&self, record: &Record, detail: &str) -> bool {
        self.registry
            .transition(
                &record.parent,
                &record.name,
                TaskState::Blocked,
                "vigilante",
                detail,
                |_| {},
            )
            .is_ok()
    }

    /// `hc read <terminal> --lines 0`, recortado a [`SCREEN_LIMIT`].
    fn read_screen(&self, terminal: &str) -> Option<String> {
        let args = vec![
            "read".to_string(),
            terminal.to_string(),
            "--lines".to_string(),
            "0".to_string(),
        ];
        let output = self.runtime.hc(&args).ok()?;
        if output.code != 0 {
            return None;
        }
        let text = if output.stdout.trim().is_empty() {
            output.stderr
        } else {
            output.stdout
        };
        Some(screen_tail(&text, SCREEN_LIMIT))
    }

    /// Guarda la pantalla y dice si cambió desde la vuelta anterior.
    ///
    /// `None` en la primera lectura de un hijo: todavía no hay vuelta anterior
    /// con la que comparar, así que no cuenta como pantalla estable (§3).
    fn observe_screen(&mut self, name: &str, screen: &str, now: u64) -> Option<bool> {
        match self.screens.get_mut(name) {
            Some(memory) => {
                if memory.screen == screen {
                    return Some(false);
                }
                memory.screen = screen.to_string();
                memory.changed_at = now;
                memory.stall_notified = false;
                Some(true)
            }
            None => {
                self.screens.insert(
                    name.to_string(),
                    ScreenMemory {
                        screen: screen.to_string(),
                        changed_at: now,
                        stall_notified: false,
                    },
                );
                None
            }
        }
    }

    fn check_stall(&mut self, record: &Record, stall_min: u64, now: u64, tick: &mut Tick) {
        let Some((elapsed, notified)) = self
            .screens
            .get(&record.name)
            .map(|memory| (now.saturating_sub(memory.changed_at), memory.stall_notified))
        else {
            return;
        };
        if notified || elapsed < stall_min.saturating_mul(60_000) {
            return;
        }
        let minutes = elapsed / 60_000;
        let text = format!(
            "[EzAgent · aviso] {} ({}) lleva {minutes} min sin avance. Revisa su pantalla: hc read {}",
            record.name,
            agent_label(record),
            record.terminal
        );
        if self.send(&text) {
            if let Some(memory) = self.screens.get_mut(&record.name) {
                memory.stall_notified = true;
            }
            tick.stalled.push(record.name.clone());
        }
    }
}

/// Un diálogo conocido en pantalla; con `working` no se considera bloqueo.
fn is_dialog(child: &Child, screen: &str) -> bool {
    child.agent_state != "working" && !matches!(classify(screen).kind, DialogKind::None)
}

/// Una entrada de `uso.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct UsageEntry {
    pub(super) agente: String,
    pub(super) modelo: Option<String>,
    pub(super) hasta: Option<u64>,
    pub(super) visto_ms: u64,
}

/// Anota en `<raíz>/uso.json` que ese agente y modelo están agotados.
///
/// DECISION: el archivo es una lista JSON en la raíz; se actualiza por
/// (agente, modelo) y se descartan las entradas ya vencidas. La escritura es
/// temporal + renombrar, sin candado: el último vigilante que escribe gana.
pub(super) fn record_usage(
    root: &Path,
    agent: &str,
    model: Option<&str>,
    until_ms: Option<u64>,
    now_ms: u64,
) -> Result<(), String> {
    let path = root.join("uso.json");
    let mut entries = read_usage(&path);
    entries.retain(|entry| entry.hasta.is_none_or(|until| until > now_ms));
    let entry = UsageEntry {
        agente: agent.to_string(),
        modelo: model.map(str::to_string),
        hasta: until_ms,
        visto_ms: now_ms,
    };
    match entries
        .iter_mut()
        .find(|other| other.agente == entry.agente && other.modelo == entry.modelo)
    {
        Some(existing) => *existing = entry,
        None => entries.push(entry),
    }
    write_usage(&path, &entries)
}

fn read_usage(path: &Path) -> Vec<UsageEntry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_usage(path: &Path, entries: &[UsageEntry]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("no pude serializar uso.json: {error}"))?;
    let folder = path
        .parent()
        .ok_or_else(|| "uso.json no tiene carpeta".to_string())?;
    std::fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    let tmp = folder.join(format!(
        ".uso-{}-{}.tmp",
        std::process::id(),
        USAGE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&tmp, format!("{text}\n"))
        .map_err(|error| format!("{}: {error}", tmp.display()))?;
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {error}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
