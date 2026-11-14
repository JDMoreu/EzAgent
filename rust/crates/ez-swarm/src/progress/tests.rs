//! Tests of the progress notices.

use super::Progress;
use crate::models::Task;

fn task(id: i64, title: &str, role: &str, state: &str) -> Task {
    Task {
        id,
        title: title.to_string(),
        role: role.to_string(),
        owner: String::new(),
        state: state.to_string(),
        deps: Vec::new(),
        parent: 0,
        result: String::new(),
        rounds: 0,
        updated: 0.0,
    }
}

#[test]
fn first_view_only_takes_note() {
    let progress = Progress::new();
    assert!(progress
        .check("s1", &[task(1, "Portar", "dev", "pending")])
        .is_empty());
    assert!(progress
        .check("s1", &[task(1, "Portar", "dev", "done")])
        .is_empty());
}

#[test]
fn reports_finished_stage_and_the_next_one() {
    let progress = Progress::new();
    let pending = vec![
        task(1, "Portar", "dev", "pending"),
        task(2, "Probar", "qa", "pending"),
        task(3, "Publicar", "ops", "pending"),
    ];
    assert!(progress.check("s1", &pending).is_empty());
    let mut first = pending.clone();
    first[0].state = "done".to_string();
    first[1].deps = vec![1];
    assert_eq!(
        progress.check("s1", &first),
        vec!["Enjambre: terminó el desarrollo. Ahora empieza la validación.".to_string()]
    );
    let mut second = first.clone();
    second[1].state = "done".to_string();
    second[2].deps = vec![2];
    assert_eq!(
        progress.check("s1", &second),
        vec!["Enjambre: terminó la validación. Ahora empieza el despliegue.".to_string()]
    );
}

#[test]
fn joins_several_stages_and_keeps_running_ones() {
    let progress = Progress::new();
    let mut tasks = vec![
        task(1, "Lexer", "dev", "pending"),
        task(2, "Guía", "docs", "pending"),
        task(3, "Probar", "qa", "pending"),
    ];
    assert!(progress.check("s1", &tasks).is_empty());
    tasks[0].state = "done".to_string();
    tasks[1].state = "done".to_string();
    tasks[2].deps = vec![1];
    assert_eq!(
        progress.check("s1", &tasks),
        vec![
            "Enjambre: terminaron el desarrollo y la documentación. Ahora empieza la validación."
                .to_string()
        ]
    );
    let other = Progress::new();
    let mut running = vec![
        task(1, "Portar", "dev", "pending"),
        task(2, "Probar", "qa", "pending"),
    ];
    assert!(other.check("s1", &running).is_empty());
    running[0].state = "done".to_string();
    running[1].state = "claimed".to_string();
    assert_eq!(
        other.check("s1", &running),
        vec!["Enjambre: terminó el desarrollo. La validación sigue en curso.".to_string()]
    );
}

#[test]
fn reports_returned_tasks_with_their_round() {
    let progress = Progress::new();
    let tasks = vec![
        task(1, "Portar", "dev", "pending"),
        task(2, "Probar", "qa", "pending"),
    ];
    assert!(progress.check("s1", &tasks).is_empty());
    let mut returned = tasks.clone();
    returned[0].rounds = 3;
    assert_eq!(
        progress.check("s1", &returned),
        vec!["Enjambre: devolvieron el desarrollo para corregir (ronda 3 de 3).".to_string()]
    );
}

#[test]
fn ignores_parts_and_all_done() {
    let progress = Progress::new();
    let mut tasks = vec![
        task(1, "Portar", "dev", "pending"),
        task(2, "Probar", "qa", "pending"),
        task(3, "Lexer", "dev", "pending"),
    ];
    tasks[2].parent = 1;
    assert!(progress.check("s1", &tasks).is_empty());
    tasks[2].state = "done".to_string();
    assert!(progress.check("s1", &tasks).is_empty());
    tasks[0].state = "done".to_string();
    tasks[1].state = "done".to_string();
    assert!(progress.check("s1", &tasks).is_empty());
}

#[test]
fn forget_starts_over_and_unicode_keeps_the_title() {
    let progress = Progress::new();
    let mut tasks = vec![
        task(1, "Diseñar la «ñ» 🙂", "", "pending"),
        task(2, "Probar", "qa", "pending"),
    ];
    assert!(progress.check("s1", &tasks).is_empty());
    tasks[0].state = "done".to_string();
    assert_eq!(
        progress.check("s1", &tasks),
        vec!["Enjambre: terminó «Diseñar la «ñ» 🙂».".to_string()]
    );
    progress.forget("s1");
    assert!(progress.check("s1", &tasks).is_empty());
}
