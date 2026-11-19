//! Pruebas de la línea de comandos de PowerShell.
//!
//! El programa va citado cuando su ruta no es «simple»; en PowerShell eso
//! necesita el operador `&` para que el hijo arranque.

use serde_json::{Map, Value};

use super::*;

/// Una línea con el entorno y el PATH de siempre.
fn plain(argv: &[&str]) -> String {
    let argv: Vec<String> = argv.iter().map(|part| part.to_string()).collect();
    command_line(&argv, &Map::new(), None)
}

#[test]
fn un_programa_simple_no_necesita_el_operador() {
    assert_eq!(
        plain(&["claude", "--model", "sonnet"]),
        "claude --model sonnet"
    );
}

#[test]
fn un_programa_citado_con_espacios_usa_el_operador() {
    assert_eq!(
        plain(&["C:\\Program Files\\Agente\\agente.exe", "--echo"]),
        "& 'C:\\Program Files\\Agente\\agente.exe' --echo"
    );
}

#[test]
fn un_programa_citado_con_barras_normales_usa_el_operador() {
    assert_eq!(
        plain(&["C:\\tmp\\agente.exe", "tarea"]),
        "& 'C:\\tmp\\agente.exe' tarea"
    );
}

#[test]
fn solo_el_programa_lleva_el_operador() {
    let line = plain(&["C:\\tmp\\agente.exe", "un argumento con espacios"]);
    assert_eq!(line, "& 'C:\\tmp\\agente.exe' 'un argumento con espacios'");
}

#[test]
fn el_entorno_va_antes_del_programa_citado() {
    let argv = vec!["C:\\tmp\\agente.exe".to_string()];
    let mut env = Map::new();
    env.insert("OPENCODE_CONFIG_CONTENT".to_string(), Value::from("{}"));
    let line = command_line(&argv, &env, None);
    assert_eq!(
        line,
        "$env:OPENCODE_CONFIG_CONTENT='{}'; & 'C:\\tmp\\agente.exe'"
    );
}

#[test]
fn sin_programo_no_hay_operador() {
    assert_eq!(plain(&[]), "");
}
