//! Prueba de punta a punta de `ez delegate` con Herdr real y agentes falsos.
//!
//! La prueba no usa modelos: cada tipo del catálogo es un agente falso
//! (`fake-ws-agent --echo`) que anota su argv, su entorno y lo que recibe.
//! Lo que se comprueba es el camino del producto: `ez delegate` valida el
//! tipo, el modelo y el esfuerzo, lanza el hijo bajo su padre en Herdr, lo
//! ubica (árbol, pestaña o panel), le entrega el primer mensaje y avisa al
//! padre cuando el hijo termina, falla o se valida (§11 y §13 de
//! `docs/designs/subagentes-herdr.md`).
//!
//! Se ejecuta así:
//!
//! ```text
//! cargo build -p ez-ctl --bin ez -p ez-fakes --bin fake-ws-agent
//! cargo run -p ez-ctl --example e2e_delegate
//! ```
//!
//! Sale con 0 si todo pasó, 1 si algo falló y 2 si falta Herdr o un binario.
//! Con `EZ_E2E_REAL=1` corre además el humo con los CLI reales (§9 del brief),
//! que sí abre interfaces de verdad: solo debe correr con el permiso de la
//! persona.

// DECISION: los módulos viven en `examples/e2e_delegate/`; como la raíz del
// ejemplo es `examples/e2e_delegate.rs`, Rust los buscaría en `examples/`, así
// que cada uno dice dónde está.
#[path = "e2e_delegate/checks.rs"]
mod checks;
#[path = "e2e_delegate/children.rs"]
mod children;
#[path = "e2e_delegate/cleanup.rs"]
mod cleanup;
#[path = "e2e_delegate/placement.rs"]
mod placement;
#[path = "e2e_delegate/proc.rs"]
mod proc;
#[path = "e2e_delegate/proxy.rs"]
mod proxy;
#[path = "e2e_delegate/rejects.rs"]
mod rejects;
#[path = "e2e_delegate/setup.rs"]
mod setup;
#[path = "e2e_delegate/smoke.rs"]
mod smoke;
#[path = "e2e_delegate/state.rs"]
mod state;

use std::path::PathBuf;
use std::process::ExitCode;

use checks::Report;
use serde_json::json;

/// El nombre del archivo con el resultado, dentro de `rust-migration/reports`.
///
/// DECISION: la corrida de HD-9 va a su propio archivo: el de HD-6 es la
/// evidencia de aquella prueba y no se pisa.
const REPORT_FILE: &str = "HD-9-e2e.json";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    // El agente falso entrega su estado con `ez report`: el ejemplo hace de
    // lanzador para que el subproceso herede el entorno temporal.
    if let Some(code) = proxy::maybe_run(&args) {
        return ExitCode::from(code as u8);
    }
    ExitCode::from(run_test() as u8)
}

/// La prueba completa.
fn run_test() -> i32 {
    let mut report = Report::default();
    let sandbox = match setup::prepare() {
        Ok(sandbox) => sandbox,
        Err(error) => {
            eprintln!("[EzAgent] la prueba no puede empezar: {}", error.message());
            return 2;
        }
    };
    println!("[EzAgent] prueba {}", sandbox.id);
    if let Err(message) = children::app_json(&sandbox, &["ls".to_string()]) {
        eprintln!(
            "[EzAgent] la ventana de Herdr no responde (`herdr-app ls`): {message}\n\
             ¿Está abierta la ventana de Herdr?"
        );
        setup::remove_root(&sandbox);
        return 2;
    }
    let parent = match children::open_parent(&sandbox, &mut report) {
        Some(parent) => parent,
        None => {
            setup::remove_root(&sandbox);
            return 1;
        }
    };
    let kids = children::delegate_all(&sandbox, &parent, &mut report);
    children::check_children(&sandbox, &parent, &kids, &mut report);
    placement::check(&sandbox, &parent, &kids, &mut report);
    rejects::run(&sandbox, &parent, &mut report);
    state::run(&sandbox, &parent, &kids, &mut report);
    if std::env::var_os("EZ_E2E_REAL").is_some() {
        println!("[EzAgent] humo real (EZ_E2E_REAL=1)");
        smoke::run(&sandbox, &parent, &mut report);
    }
    // Las líneas del padre se copian antes de la limpieza (que borra la
    // carpeta de la prueba).
    report.extra.insert(
        "parent_lines".to_string(),
        json!(state::parent_lines(&sandbox, &parent)),
    );
    cleanup::run(&sandbox, &parent, &mut report);
    report.extra.insert("id".to_string(), json!(sandbox.id));
    report
        .extra
        .insert("parent".to_string(), children::parent_value(&parent));
    report.extra.insert(
        "children".to_string(),
        json!(kids.iter().map(children::to_value).collect::<Vec<_>>()),
    );
    let value = report.to_value();
    write_result(&sandbox, &value);
    println!("{}", report.table());
    if report.ok() {
        0
    } else {
        1
    }
}

/// Escribe el JSON del resultado y lo copia al informe del brief.
fn write_result(sandbox: &setup::Sandbox, value: &serde_json::Value) {
    let path = setup::result_path(sandbox);
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    if let Err(error) = std::fs::write(&path, text) {
        eprintln!("[EzAgent] no pude escribir {}: {error}", path.display());
        return;
    }
    println!("[EzAgent] resultado: {}", path.display());
    let Some(folder) = reports_folder() else {
        eprintln!("[EzAgent] no encontré la carpeta `rust-migration/reports`.");
        return;
    };
    let target = folder.join(REPORT_FILE);
    match std::fs::copy(&path, &target) {
        Ok(_) => println!("[EzAgent] copia: {}", target.display()),
        Err(error) => eprintln!("[EzAgent] no pude copiar a {}: {error}", target.display()),
    }
}

/// La carpeta `rust-migration/reports` del repositorio (sube desde donde está).
fn reports_folder() -> Option<PathBuf> {
    if let Ok(folder) = std::env::var("EZ_E2E_REPORTS") {
        let path = PathBuf::from(folder);
        if path.is_dir() {
            return Some(path);
        }
    }
    let mut current = std::env::current_dir().ok()?;
    for _ in 0..6 {
        let candidate = current.join("rust-migration").join("reports");
        if candidate.is_dir() {
            return Some(candidate);
        }
        current = current.parent()?.to_path_buf();
    }
    None
}
