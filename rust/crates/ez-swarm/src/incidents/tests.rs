//! Tests of the incident classification and texts.

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};

use super::*;
use crate::models::Member;

fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2025, 1, day)
        .and_then(|date| date.and_hms_opt(hour, minute, 0))
        .unwrap_or_else(|| panic!("fecha de prueba"))
}

fn after(text: &str) -> f64 {
    let now = at(1, 20, 0);
    let resets = parse_reset(text, now);
    if resets == 0.0 {
        0.0
    } else {
        resets - epoch(now)
    }
}

fn delta(resets: f64, now: NaiveDateTime) -> f64 {
    if resets == 0.0 {
        0.0
    } else {
        resets - epoch(now)
    }
}

fn epoch(moment: NaiveDateTime) -> f64 {
    Local
        .from_local_datetime(&moment)
        .earliest()
        .map_or(0.0, |local| local.timestamp() as f64)
}

fn member(name: &str, agent: &str, role: &str) -> Member {
    Member {
        name: name.to_string(),
        role: role.to_string(),
        agent: agent.to_string(),
        address: String::new(),
        state: "idle".to_string(),
        seen: 0.0,
        model: String::new(),
        effort: String::new(),
    }
}

fn incident(kind: &str, resets: f64) -> Incident {
    Incident {
        id: 1,
        swarm: "s1".to_string(),
        member: "bo".to_string(),
        kind: kind.to_string(),
        detail: "detalle".to_string(),
        resets,
        state: "open".to_string(),
        action: String::new(),
        created: epoch(at(1, 20, 0)),
        updated: 0.0,
    }
}

#[test]
fn reset_times_match_python() {
    for (text, expected) in [
        ("resets 7:10pm", 83400.0),
        ("resets 7:10 pm", 83400.0),
        ("resets 19:10", 83400.0),
        ("resets 7:10 a.m.", 40200.0),
        ("resets 12:30am", 16200.0),
        ("try again in 2h 30m", 9000.0),
        ("try again in 45m", 2700.0),
        ("in 3 hours", 10800.0),
        ("sin hora conocida", 0.0),
        ("resets 20:00", 86400.0),
        ("vuelve a las 21:05", 3900.0),
        ("resets 7pm", 82800.0),
        ("resets 12:00pm", 57600.0),
        ("resets 12:00am", 14400.0),
        ("try again at 11:59pm", 14340.0),
        ("resets 6:05 p.m.", 79500.0),
        ("available 20:15", 900.0),
        ("disponible a las 22:00", 7200.0),
        ("vuelve en 90m", 5400.0),
        ("resets 25:00", 0.0),
    ] {
        assert_eq!(after(text), expected, "{text}");
    }
}

#[test]
fn classification_labels_quota_and_context() {
    let now = at(1, 20, 0);
    for (screen, kind, detail, resets) in [
        (
            "You've hit your 5-hour limit · resets 7:10pm",
            "quota",
            "You've hit your 5-hour limit · resets 7:10pm",
            83400.0,
        ),
        (
            "Too many requests (429). Try again in 2h 30m.",
            "quota",
            "Too many requests (429). Try again in 2h 30m.",
            9000.0,
        ),
        (
            "Se alcanzó el límite de uso del plan",
            "quota",
            "Se alcanzó el límite de uso del plan",
            0.0,
        ),
        (
            "Sin créditos disponibles",
            "quota",
            "Sin créditos disponibles",
            0.0,
        ),
        (
            "Rate limited by the provider",
            "quota",
            "Rate limited by the provider",
            0.0,
        ),
        ("Prompt is too long", "context", "Prompt is too long", 0.0),
        (
            "maximum context length exceeded: 200000",
            "context",
            "maximum context length exceeded: 200000",
            0.0,
        ),
        (
            "hit your usage limit, available 3:00pm",
            "quota",
            "hit your usage limit, available 3:00pm",
            68400.0,
        ),
    ] {
        let signal = classify(screen, now).unwrap_or_else(|| panic!("sin señal: {screen}"));
        assert_eq!(signal.kind, kind, "{screen}");
        assert_eq!(signal.detail, detail, "{screen}");
        assert_eq!(delta(signal.resets, now), resets, "{screen}");
    }
}

#[test]
fn classification_labels_auth_and_compacting() {
    let now = at(1, 20, 0);
    for (screen, kind) in [
        ("Not logged in · Please run /login", "auth"),
        ("401 Unauthorized", "auth"),
        ("sesión expirada, vuelve a entrar", "auth"),
        ("Compacting conversation…", "compacting"),
        (
            "Compacting the conversation before the next turn",
            "compacting",
        ),
        ("context window is full", "context"),
    ] {
        let signal = classify(screen, now).unwrap_or_else(|| panic!("sin señal: {screen}"));
        assert_eq!(signal.kind, kind, "{screen}");
        assert_eq!(signal.detail, screen, "{screen}");
    }
}

#[test]
fn classification_priority_tail_and_detail_clip() {
    let now = at(1, 20, 0);
    let priority = classify(
        "quota exceeded
401 Unauthorized",
        now,
    )
    .unwrap_or_else(|| panic!("sin señal"));
    assert_eq!(priority.kind, "auth");
    assert!(classify("todo bien, sigue trabajando", now).is_none());
    assert!(classify("la ventana de contexto está llena", now).is_none());
    let many = format!(
        "usage limit
{}",
        (0..24)
            .map(|index| format!("linea {index}"))
            .collect::<Vec<_>>()
            .join(
                "
"
            )
    );
    assert!(classify(&many, now).is_none());
    let spaced =
        classify("  hit   your   usage   limit  ", now).unwrap_or_else(|| panic!("sin señal"));
    assert_eq!(spaced.detail, "hit your usage limit");
    let long = format!("hit your usage limit {}", "x".repeat(300));
    let clipped = classify(&long, now).unwrap_or_else(|| panic!("sin señal"));
    assert_eq!(clipped.detail.chars().count(), 200);
}

#[test]
fn spoken_times_use_spanish_clock() {
    let reference = epoch(at(1, 20, 0));
    for (day, hour, minute, text) in [
        (1, 0, 0, "las 12:00 a. m."),
        (2, 0, 0, "mañana a las 12:00 a. m."),
        (1, 6, 0, "las 6:00 a. m."),
        (1, 12, 0, "las 12:00 p. m."),
        (2, 12, 0, "mañana a las 12:00 p. m."),
        (1, 13, 0, "las 1:00 p. m."),
        (2, 19, 0, "mañana a las 7:00 p. m."),
    ] {
        assert_eq!(when(epoch(at(day, hour, minute)), reference), text);
    }
    assert_eq!(when(0.0, reference), "");
}

#[test]
fn suggestions_skip_the_failed_member_and_busy_ones() {
    let members = [
        member("ana", "claude", "plan"),
        member("bo", "opencode", "dev"),
        member("cy", "codex", "qa"),
    ];
    let mut busy = HashSet::new();
    busy.insert("bo".to_string());
    assert_eq!(
        suggest(&members, &members[0], &busy).map(|item| item.name.as_str()),
        Some("cy")
    );
    let mut all_busy = busy.clone();
    all_busy.insert("cy".to_string());
    assert!(suggest(&members, &members[0], &all_busy).is_none());
}

#[test]
fn describe_and_board_note_use_the_kind_label() {
    let me = member("bo", "opencode", "dev");
    let tasks = ["T1".to_string(), "T2".to_string()];
    assert_eq!(
        describe(&incident("quota", epoch(at(2, 9, 0))), &me, &tasks),
        "bo (opencode, desarrollar) se quedó sin uso hasta mañana a las 9:00 a. m.. Su trabajo pendiente: T1, T2."
    );
    assert_eq!(
        board_note(&incident("auth", 0.0), &me, &[]),
        "bo (opencode) perdió la sesión con su proveedor."
    );
    assert_eq!(
        board_note(&incident("closed", 0.0), &me, &tasks),
        "bo (opencode) tiene su ventana cerrada; T1, T2 en espera."
    );
    assert_eq!(
        board_note(&incident("raro", 0.0), &me, &[]),
        "bo (opencode) raro."
    );
}

#[test]
fn questions_offer_wait_swap_or_pause() {
    let me = member("bo", "opencode", "dev");
    let other = member("cy", "codex", "qa");
    assert_eq!(
        question(&incident("quota", 0.0), &me, &[], None),
        "Enjambre: bo (opencode, desarrollar) se quedó sin uso. ¿Espero a que vuelva, lo reemplazo con otro agente o pauso el enjambre?"
    );
    assert_eq!(
        question(&incident("quota", epoch(at(2, 9, 0))), &me, &[], Some(&other)),
        "Enjambre: bo (opencode, desarrollar) se quedó sin uso hasta mañana a las 9:00 a. m.. ¿Espero a que se restablezca, lo reemplazo con codex o pauso el enjambre?"
    );
    assert_eq!(
        question(&incident("auth", 0.0), &me, &[], None),
        "Enjambre: bo (opencode, desarrollar) perdió la sesión con su proveedor. Inicia sesión en su ventana y lo retomo, o dime si lo reemplazo con otro agente."
    );
    assert_eq!(
        question(&incident("closed", 0.0), &me, &[], Some(&other)),
        "Enjambre: bo (opencode, desarrollar) tiene su ventana cerrada. ¿Lo reabro, lo reemplazo con codex o pauso el enjambre?"
    );
    assert_eq!(
        question(&incident("raro", 0.0), &me, &[], None),
        "Enjambre: bo (opencode, desarrollar) raro. ¿Lo reabro, lo reemplazo con otro agente o pauso el enjambre?"
    );
}
