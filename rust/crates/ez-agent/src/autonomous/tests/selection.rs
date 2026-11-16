//! Selección de herramientas para el modelo (10.5).

use crate::runtime::{select_tool_names, SelectOptions, BASE_TOOLS};

fn contains(names: &[String], name: &str) -> bool {
    names.iter().any(|picked| picked == name)
}

#[test]
fn the_start_tools_need_an_authorized_message() {
    let plain = select_tool_names("haz esa tarea", &SelectOptions::default());
    assert!(!contains(&plain, "start_autonomous_task"));
    assert!(!contains(&plain, "start_autonomous_team"));

    let authorized = select_tool_names(
        "haz esa tarea",
        &SelectOptions {
            autonomous_authorized: true,
            ..SelectOptions::default()
        },
    );
    assert!(contains(&authorized, "start_autonomous_task"));
    assert!(contains(&authorized, "start_autonomous_team"));
    assert!(!contains(&authorized, "autonomous_status"));
    assert!(!contains(&authorized, "stop_autonomous_task"));
    assert!(!contains(&authorized, "resume_autonomous_task"));
}

#[test]
fn status_stop_and_resume_are_offered_while_a_task_exists() {
    let active = select_tool_names(
        "¿cómo va?",
        &SelectOptions {
            autonomous_active: true,
            ..SelectOptions::default()
        },
    );
    assert!(contains(&active, "autonomous_status"));
    assert!(contains(&active, "stop_autonomous_task"));
    assert!(contains(&active, "resume_autonomous_task"));
    assert!(!contains(&active, "start_autonomous_task"));

    let both = select_tool_names(
        "para y luego sigue",
        &SelectOptions {
            autonomous_authorized: true,
            autonomous_active: true,
            ..SelectOptions::default()
        },
    );
    for name in [
        "start_autonomous_task",
        "start_autonomous_team",
        "autonomous_status",
        "stop_autonomous_task",
        "resume_autonomous_task",
    ] {
        assert!(contains(&both, name), "{name}");
    }
}

#[test]
fn the_autonomous_tools_do_not_crowd_out_the_base_ones() {
    let names = select_tool_names(
        "hazlo en modo autónomo",
        &SelectOptions {
            autonomous_authorized: true,
            autonomous_active: true,
            ..SelectOptions::default()
        },
    );
    for base in BASE_TOOLS {
        assert!(contains(&names, base), "{base}");
    }
    assert!(names.len() <= 14);
}
