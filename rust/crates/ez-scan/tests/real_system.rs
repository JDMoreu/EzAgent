//! Prueba real acotada: solo que los números del equipo sean positivos.
//!
//! No abre agentes ni toca la configuración del usuario: mide el sistema y los
//! recursos de este proceso. Nada aquí se acerca a 1 GB.

use std::path::Path;

#[test]
fn el_sistema_real_da_numeros_positivos() {
    let workdir = std::env::temp_dir();
    let info = ez_scan::system(&workdir).expect("medir el sistema");
    assert!(!info.os.trim().is_empty(), "sin nombre de SO");
    assert!(!info.cpu_model.trim().is_empty(), "sin modelo de CPU");
    assert!(info.physical_cores > 0, "sin núcleos físicos");
    assert!(info.logical_cores >= info.physical_cores);
    assert!(info.total_ram_mb > 0, "sin RAM total");
    assert!(info.disk_free_mb > 0, "sin disco libre");
    for gpu in &info.gpus {
        assert!(!gpu.name.trim().is_empty());
    }
}

#[test]
fn los_recursos_reales_se_pueden_medir() {
    let resources = ez_scan::resources("", &std::env::temp_dir()).expect("medir recursos");
    assert!(resources.free_ram_mb > 0, "sin RAM libre");
    assert!(resources.free_commit_mb >= resources.free_ram_mb);
    assert!(resources.cpu_percent >= 0.0);
    assert!(resources.disk_free_mb > 0);
    assert!(resources.subagents.is_empty());
}

#[test]
fn el_disco_de_una_carpeta_inexistente_no_rompe() {
    let info =
        ez_scan::system(Path::new("C:\\carpeta-que-no-existe-ez-scan")).expect("medir el sistema");
    assert!(info.total_ram_mb > 0);
}
