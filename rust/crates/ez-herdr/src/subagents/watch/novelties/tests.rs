//! Pruebas del detector de novedades: patrones, horas y `uso.json`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Local, TimeZone};

use super::*;

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ez-novedades-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("carpeta temporal");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

const NOW: u64 = 1_700_000_000_000;

#[test]
fn factory_patterns_match_without_case() {
    let novelties = Novelties::default();
    let hit = novelties
        .limit_hit("opencode", "FREE LIMIT REACHED · free plan", NOW)
        .expect("límite");
    assert_eq!(hit.detail(), "límite de uso del plan gratuito");

    let hit = novelties
        .limit_hit("codex", "You've hit your usage limit", NOW)
        .expect("límite");
    assert_eq!(hit.detail(), "límite de uso");

    assert!(novelties
        .limit_hit("opencode", "todo tranquilo", NOW)
        .is_none());
}

#[test]
fn single_words_are_not_a_limit() {
    let novelties = Novelties::default();
    for text in ["quota", "rate limit", "usage limit", "resets at 7:00 p.m."] {
        assert!(
            novelties.limit_hit("opencode", text, NOW).is_none(),
            "«{text}» no es un límite completo"
        );
    }
    assert!(novelties
        .limit_hit("opencode", "quota exceeded", NOW)
        .is_some());
    assert!(novelties
        .limit_hit("opencode", "Usage limit reached", NOW)
        .is_some());
}

#[test]
fn limit_patterns_only_look_at_the_last_fifteen_lines() {
    let novelties = Novelties::default();
    let folded = "trabajando\n".repeat(20);
    let top = format!("Free limit reached. Resets at 7:00 p.m.\n{folded}");
    assert!(
        novelties.limit_hit("opencode", &top, NOW).is_none(),
        "la frase en la parte alta no cuenta"
    );

    let bottom = format!("{folded}Free limit reached. Resets at 7:00 p.m.");
    let hit = novelties
        .limit_hit("opencode", &bottom, NOW)
        .expect("límite al final");
    assert!(hit.until_ms.is_some());
}

#[test]
fn last_lines_keeps_the_tail_with_text() {
    assert_eq!(last_lines("a\nb\nc", 2), "b\nc");
    assert_eq!(last_lines("a", 0), "");
    assert_eq!(last_lines("uno\n\n┃\n╹▀▀▀\ndos", 2), "uno\ndos");
    assert_eq!(last_lines("┃\n╹▀▀▀\n", 5), "");
    let long: String = (0..20).map(|index| format!("l{index}\n")).collect();
    let tail = last_lines(&long, LIMIT_TAIL_LINES);
    assert_eq!(tail.lines().count(), LIMIT_TAIL_LINES);
    assert!(!tail.contains("l0"));
    assert!(tail.contains("l19"));
}

#[test]
fn file_overrides_patterns_and_stall() {
    let root = TempRoot::new();
    let file = root.path().join("novedades.toml");
    std::fs::write(
        &file,
        "# patrones\nstall_min = 3\nopencode = free limit reached\n* = quota\n",
    )
    .expect("escribir");

    let novelties = Novelties::load(Some(&file));
    assert_eq!(novelties.stall_min, 3);
    assert!(novelties
        .limit_hit("opencode", "FREE LIMIT REACHED", NOW)
        .is_some());
    assert!(novelties
        .limit_hit("codex", "free limit reached", NOW)
        .is_none());
    assert!(novelties.limit_hit("codex", "quota agotada", NOW).is_some());
}

#[test]
fn an_empty_file_keeps_the_factory_patterns() {
    let root = TempRoot::new();
    let file = root.path().join("novedades.toml");
    std::fs::write(&file, "# solo comentarios\n").expect("escribir");

    let novelties = Novelties::load(Some(&file));
    assert_eq!(novelties.stall_min, DEFAULT_STALL_MIN);
    assert!(novelties
        .limit_hit("opencode", "Free limit reached", NOW)
        .is_some());
}

#[test]
fn reset_at_a_clock_and_in_a_delta() {
    let at = reset_time_ms("Resets at 7:00 p.m.", NOW).expect("hora");
    let local = Local
        .timestamp_millis_opt(i64::try_from(at).expect("milisegundos"))
        .single()
        .expect("fecha local");
    assert_eq!(local.format("%H:%M").to_string(), "19:00");
    assert!(at > NOW);

    assert_eq!(
        reset_time_ms("resets in 2h", NOW),
        Some(NOW + 2 * 3_600_000)
    );
    assert_eq!(
        reset_time_ms("resets in 1h 30m", NOW),
        Some(NOW + 5_400_000)
    );
    assert_eq!(reset_time_ms("resets in 45m", NOW), Some(NOW + 2_700_000));
    assert_eq!(reset_time_ms("sin reinicio", NOW), None);
}

#[test]
fn screen_tail_keeps_the_last_characters() {
    assert_eq!(screen_tail("abc", 10), "abc");
    assert_eq!(screen_tail("abcdef", 3), "def");
    assert_eq!(screen_tail("abc", 0), "");
    let long = "x".repeat(SCREEN_LIMIT + 10);
    assert_eq!(
        screen_tail(&long, SCREEN_LIMIT).chars().count(),
        SCREEN_LIMIT
    );
}

#[test]
fn usage_ledger_upserts_by_agent_and_model_and_prunes() {
    let root = TempRoot::new();
    record_usage(root.path(), "opencode", Some("m1"), Some(NOW + 2_000), NOW).expect("anotar");
    record_usage(
        root.path(),
        "opencode",
        Some("m1"),
        Some(NOW + 3_000),
        NOW + 1,
    )
    .expect("actualizar");
    record_usage(root.path(), "codex", None, Some(NOW - 1_000), NOW - 9_000)
        .expect("entrada vieja");

    let entries = read_usage(&root.path().join("uso.json"));
    assert_eq!(entries.len(), 2);
    let opencode = entries
        .iter()
        .find(|entry| entry.agente == "opencode")
        .expect("opencode");
    assert_eq!(opencode.hasta, Some(NOW + 3_000));
    assert_eq!(opencode.visto_ms, NOW + 1);

    record_usage(
        root.path(),
        "opencode",
        Some("m1"),
        Some(NOW + 4_000),
        NOW + 2,
    )
    .expect("prune");
    let entries = read_usage(&root.path().join("uso.json"));
    assert_eq!(entries.len(), 1, "la entrada vencida se descarta");
    assert_eq!(entries[0].hasta, Some(NOW + 4_000));
}
