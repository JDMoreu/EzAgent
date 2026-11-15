//! Servicios falsos de `ez-fakes` lanzados en puertos libres.
//!
//! Igual que `Services` de `parity/replay/run_py.py`: cada corrida escribe su
//! JSON de escenario, pide un puerto al sistema y termina los procesos siempre
//! al acabar (también si el guion falla).

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ez_core::pyjson::{dumps_with, DumpOptions};
use serde_json::Value;

/// Procesos falsos de una corrida.
pub struct Services {
    fakes: PathBuf,
    procs: Vec<Child>,
}

impl Services {
    /// Servicios que buscan sus binarios en `fakes`.
    pub fn new(fakes: impl Into<PathBuf>) -> Self {
        Self {
            fakes: fakes.into(),
            procs: Vec::new(),
        }
    }

    /// Arranca `fake-<name>` con el bloque JSON del escenario y devuelve el puerto.
    pub fn start(&mut self, name: &str, scenario: &Value, home: &Path) -> Result<u16, String> {
        let exe = self.fakes.join(format!("{name}.exe"));
        if !exe.is_file() {
            return Err(format!(
                "falta {}; compílalo con: cargo build -p ez-fakes --bins",
                exe.display()
            ));
        }
        let scenario_path = home.join(format!("{name}.json"));
        std::fs::write(&scenario_path, dumps_with(scenario, &DumpOptions::utf8()))
            .map_err(|error| format!("no pude escribir {}: {error}", scenario_path.display()))?;
        let port = free_port()?;
        let mut command = Command::new(&exe);
        command
            .arg("--scenario")
            .arg(&scenario_path)
            .arg("--port")
            .arg(port.to_string());
        if name == "fake-llm" {
            command.arg("--record").arg(home.join("requests.jsonl"));
        }
        let child = command
            .current_dir(&self.fakes)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("no pude arrancar {}: {error}", exe.display()))?;
        self.procs.push(child);
        wait_port(port, 15.0)?;
        Ok(port)
    }

    /// Termina todos los procesos (siempre en el `finally` del Python).
    pub fn stop(&mut self) {
        for child in &mut self.procs {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.procs.clear();
    }
}

impl Drop for Services {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Puerto libre en `127.0.0.1` (`free_port` del Python).
pub fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("no pude pedir un puerto: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("sin dirección local: {error}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// Espera a que el servicio acepte conexiones (`wait_port`).
pub fn wait_port(port: u16, timeout: f64) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}")
                .parse()
                .map_err(|error| format!("dirección no válida: {error}"))?,
            Duration::from_millis(200),
        )
        .is_ok()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("el servicio falso no abrió el puerto {port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_ports_are_usable_and_distinct() {
        let first = free_port().expect("puerto");
        let second = free_port().expect("puerto");
        assert_ne!(first, second);
        assert!(TcpListener::bind(("127.0.0.1", first)).is_ok());
    }

    #[test]
    fn a_port_without_service_times_out() {
        let port = free_port().expect("puerto");
        assert!(wait_port(port, 0.2).is_err());
    }
}
