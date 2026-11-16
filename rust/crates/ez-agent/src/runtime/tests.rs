//! Pruebas del runtime (F4-09): selección de herramientas por intención.

mod execute;

use std::collections::HashSet;

use super::*;

fn names(text: &str, options: &SelectOptions<'_>) -> Vec<String> {
    select_tool_names(text, options)
}

#[test]
fn base_tools_are_five_and_each_intent_adds_without_removing() {
    let base = names("hola qué tal", &SelectOptions::default());
    assert_eq!(base, BASE_TOOLS.map(str::to_string));
    let names_with_time = names(
        "¿qué hora es? y copia el informe",
        &SelectOptions::default(),
    );
    assert!(names_with_time.contains(&"get_datetime".to_string()));
    assert!(names_with_time.contains(&"copy_path".to_string()));
    for base in BASE_TOOLS {
        assert!(names_with_time.contains(&base.to_string()));
    }
    assert!(names_with_time.len() <= 14);
}

#[test]
fn documents_skills_memory_scripts_and_the_optional_recall() {
    let documents = names("convierte este informe a PDF", &SelectOptions::default());
    assert!(documents.contains(&"load_skill".to_string()));
    let excel = names(
        "haz una hoja de Excel con estos datos",
        &SelectOptions::default(),
    );
    assert!(excel.contains(&"load_skill".to_string()));
    let word = names("pásalo a Word", &SelectOptions::default());
    assert!(word.contains(&"load_skill".to_string()));
    let manual = names("usa la skill pdf", &SelectOptions::default());
    assert!(manual.contains(&"load_skill".to_string()));
    assert!(manual.contains(&"run_skill_script".to_string()));
    let memory = names("recuerda esto en memoria", &SelectOptions::default());
    for expected in ["remember", "recall", "forget", "compact_memory"] {
        assert!(memory.contains(&expected.to_string()));
    }
    let without_memory = names(
        "recuerda en memoria",
        &SelectOptions {
            has_memory: false,
            ..SelectOptions::default()
        },
    );
    assert!(!without_memory.contains(&"recall".to_string()));
    let scripts = names(
        "hola",
        &SelectOptions {
            scripts_loaded: true,
            ..SelectOptions::default()
        },
    );
    assert!(scripts.contains(&"run_skill_script".to_string()));
    assert!(scripts.contains(&"fetch_page".to_string()));
}

#[test]
fn file_and_undo_intents_offer_their_tool() {
    let cases = [
        ("escribe el resumen", "write_file"),
        ("escribe el resumen", "create_dir"),
        ("abre el documento", "open_path"),
        ("borra la carpeta", "delete_path"),
        ("lista la carpeta", "list_dir"),
        ("muéstrame la carpeta", "list_dir"),
        ("mueve el archivo a la carpeta X", "move_path"),
        ("renombra el informe", "move_path"),
        ("deshaz lo último", "undo_last"),
        ("revierte el cambio", "undo_last"),
    ];
    for (text, expected) in cases {
        let picked = names(text, &SelectOptions::default());
        assert!(picked.contains(&expected.to_string()), "{text}: {picked:?}");
        for base in BASE_TOOLS {
            assert!(picked.contains(&base.to_string()), "{text}");
        }
        assert!(picked.len() <= 14, "{text}");
    }
}

#[test]
fn skill_keywords_offer_load_skill() {
    let keywords: HashSet<String> = ["chrome".to_string(), "navegador".to_string()]
        .into_iter()
        .collect();
    let offered = names(
        "abre Chrome y entra a Gmail",
        &SelectOptions {
            skill_keywords: Some(&keywords),
            ..SelectOptions::default()
        },
    );
    assert!(offered.contains(&"load_skill".to_string()));
    let plain = names(
        "hola qué tal",
        &SelectOptions {
            skill_keywords: Some(&keywords),
            ..SelectOptions::default()
        },
    );
    assert_eq!(plain, BASE_TOOLS.map(str::to_string));
}

#[test]
fn swarm_needs_two_distinct_roles_or_explicit_words() {
    for text in [
        "trabajen juntos en equipo",
        "el rol de cada agente",
        "un planificador y un qa",
        "haz un plan y que lo revise otro",
    ] {
        assert!(
            names(text, &SelectOptions::default()).contains(&"swarm".to_string()),
            "{text}"
        );
    }
    for text in [
        "abre claude y dile que revise el README",
        "¿qué hora es?",
        "qa qa",
    ] {
        assert!(
            !names(text, &SelectOptions::default()).contains(&"swarm".to_string()),
            "{text}"
        );
    }
    let active = names(
        "¿cómo va?",
        &SelectOptions {
            swarm_active: true,
            ..SelectOptions::default()
        },
    );
    assert!(active.contains(&"swarm".to_string()));
}

#[test]
fn never_more_than_fourteen_names() {
    let text = "hoy escribe, copia, abre, borra, mueve, lista y deshaz; usa la skill y \
                recuerda en memoria; planifica y que el auditor revise";
    for scripts in [false, true] {
        let picked = names(
            text,
            &SelectOptions {
                scripts_loaded: scripts,
                ..SelectOptions::default()
            },
        );
        assert!(picked.len() <= 14);
        let unique: HashSet<&String> = picked.iter().collect();
        assert_eq!(unique.len(), picked.len());
    }
}
