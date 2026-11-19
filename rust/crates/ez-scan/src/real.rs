//! El medidor real: `sysinfo` para el equipo y `hc`/`herdr-app` para Herdr.

mod commit;

use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use serde_json::Value;
use sysinfo::{
    CpuRefreshKind, Disks, Gpus, MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate,
    RefreshKind, System,
};

use crate::probe::Probe;
use crate::resources::{bytes_to_mb, SubagentUsage};
use crate::system::{GpuInfo, SystemInfo};

/// Lo que mide de verdad esta máquina.
pub struct RealProbe;

impl Probe for RealProbe {
    fn system(&self, workdir: &Path) -> Result<SystemInfo, String> {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        )
        .map_err(|error| format!("No pude medir el sistema: {error}"))?;
        system.refresh_memory();
        let logical_cores = system.cpus().len();
        let cpu_model = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .unwrap_or_default();
        let physical_cores = System::physical_core_count().unwrap_or(logical_cores);
        let os = System::long_os_version()
            .ok()
            .or_else(|| System::name().ok())
            .unwrap_or_default();
        Ok(SystemInfo {
            os,
            os_version: System::os_version().unwrap_or_default(),
            cpu_model,
            physical_cores,
            logical_cores,
            total_ram_mb: bytes_to_mb(system.total_memory()),
            disk_free_mb: self.disk_free_mb(workdir).unwrap_or(0),
            gpus: gpus(),
            hc_version: version_of(self, "hc"),
            herdr_app_version: version_of(self, "herdr-app"),
        })
    }

    fn free_ram_mb(&self) -> Result<u64, String> {
        let mut system =
            System::new().map_err(|error| format!("No pude medir la memoria: {error}"))?;
        system.refresh_memory();
        Ok(bytes_to_mb(system.free_memory()))
    }

    /// En Windows se lee el commit real (`CommitLimit − CommitTotal`); fuera de
    /// Windows, o si la API falla, se aproxima con RAM libre + swap libre.
    fn free_commit_mb(&self) -> Result<u64, String> {
        if let Some((free, _)) = commit::free_and_total_mb() {
            return Ok(free);
        }
        let mut system =
            System::new().map_err(|error| format!("No pude medir la memoria: {error}"))?;
        system.refresh_memory();
        Ok(bytes_to_mb(system.free_memory() + system.free_swap()))
    }

    fn cpu_percent(&self) -> Result<f32, String> {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()),
        )
        .map_err(|error| format!("No pude medir la CPU: {error}"))?;
        system.refresh_cpu_all();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        system.refresh_cpu_all();
        Ok(system.global_cpu_usage())
    }

    fn disk_free_mb(&self, workdir: &Path) -> Option<u64> {
        let disks = Disks::new_with_refreshed_list().ok()?;
        let target = workdir
            .canonicalize()
            .unwrap_or_else(|_| workdir.to_path_buf())
            .to_string_lossy()
            .to_lowercase();
        let mut best: Option<(usize, u64)> = None;
        for disk in &disks {
            let mount = disk.mount_point().to_string_lossy().to_lowercase();
            if !target.starts_with(&mount) {
                continue;
            }
            let length = mount.len();
            if best.is_none_or(|(seen, _)| length >= seen) {
                best = Some((length, disk.available_space()));
            }
        }
        match best {
            Some((_, space)) => Some(bytes_to_mb(space)),
            None => disks
                .iter()
                .next()
                .map(|disk| bytes_to_mb(disk.available_space())),
        }
    }

    fn subagents(
        &self,
        parent: &str,
        thresholds: &crate::Thresholds,
    ) -> Result<Vec<SubagentUsage>, String> {
        if parent.trim().is_empty() {
            return Ok(Vec::new());
        }
        let herdr = herdr();
        let argv = vec![
            herdr.hc_path(),
            "ls".to_string(),
            "--json".to_string(),
            "--label".to_string(),
            format!("ez.parent={}", parent.trim()),
        ];
        let completed = herdr
            .run_command(&argv, 10.0, false)
            .map_err(|error| format!("No pude listar los subagentes: {error}"))?;
        let data = parse_json(&herdr.hc_path(), &completed.stdout)?;
        let Some(Value::Array(terminals)) = data.get("terminals") else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for terminal in terminals {
            let Some(object) = terminal.as_object() else {
                continue;
            };
            if object.get("state").and_then(Value::as_str) != Some("live") {
                continue;
            }
            let address = text_of(object.get("address"));
            let name = text_of(object.get("name"));
            let (memory_mb, processes) = terminal_tree(&herdr, &address).unwrap_or((0, Vec::new()));
            let reason = crate::resources::heavy_reason(memory_mb, &processes, thresholds);
            out.push(SubagentUsage {
                name: if name.is_empty() {
                    address.clone()
                } else {
                    name
                },
                terminal: address,
                agent: text_of(object.get("agent")),
                memory_mb,
                heavy: reason.is_some(),
                heavy_reason: reason,
            });
        }
        Ok(out)
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        ez_herdr::herdr::system_runtime().which(program)
    }

    fn version(&self, program: &Path) -> Option<String> {
        if !is_runnable(program) {
            return None;
        }
        let output = run(program, &["--version"], 10.0)?;
        output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
    }

    fn opencode_models(&self, program: &Path) -> Vec<String> {
        if !is_runnable(program) {
            return Vec::new();
        }
        run(program, &["models"], 10.0)
            .map(|output| {
                output
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn herdr() -> Herdr {
    Herdr::new(VoiceConfig::new())
}

fn version_of(probe: &RealProbe, program: &str) -> Option<String> {
    let path = probe.which(program)?;
    probe.version(&path)
}

fn gpus() -> Vec<GpuInfo> {
    let Ok(gpus) = Gpus::new_with_refreshed_list() else {
        return Vec::new();
    };
    (&gpus)
        .into_iter()
        .map(|gpu| GpuInfo {
            name: gpu
                .model()
                .or_else(|| gpu.vendor())
                .unwrap_or("GPU")
                .trim()
                .to_string(),
            dedicated_vram_mb: bytes_to_mb(gpu.total_memory().unwrap_or(0)),
        })
        .collect()
}

fn parse_json(program: &str, stdout: &str) -> Result<Value, String> {
    serde_json::from_str(stdout.trim())
        .map_err(|error| format!("{program} devolvió algo que no es JSON: {error}"))
}

fn text_of(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Ejecuta `hc get <terminal>` y resume su árbol: memoria y nombres.
fn terminal_tree(herdr: &Herdr, address: &str) -> Option<(u64, Vec<String>)> {
    if address.is_empty() {
        return None;
    }
    let argv = vec![
        herdr.hc_path(),
        "get".to_string(),
        address.to_string(),
        "--json".to_string(),
    ];
    let completed = herdr.run_command(&argv, 10.0, false).ok()?;
    let data = parse_json(&herdr.hc_path(), &completed.stdout).ok()?;
    let pid = u32::try_from(data.get("pid")?.as_u64()?).ok()?;
    Some(process_tree(pid))
}

fn process_tree(root: u32) -> (u64, Vec<String>) {
    let Ok(mut system) = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing().with_memory()),
    ) else {
        return (0, Vec::new());
    };
    system.refresh_processes(ProcessesToUpdate::All, true);
    let root = Pid::from_u32(root);
    let mut total: u64 = 0;
    let mut names = Vec::new();
    for (pid, process) in system.processes() {
        if *pid == root || descends_from(&system, *pid, root) {
            total = total.saturating_add(process.memory());
            names.push(process.name().to_string_lossy().into_owned());
        }
    }
    (bytes_to_mb(total), names)
}

fn descends_from(system: &System, pid: Pid, root: Pid) -> bool {
    let mut current = pid;
    for _ in 0..64 {
        let Some(process) = system.process(current) else {
            return false;
        };
        let Some(parent) = process.parent() else {
            return false;
        };
        if parent == root {
            return true;
        }
        if parent == current {
            return false;
        }
        current = parent;
    }
    false
}

fn run(program: &Path, args: &[&str], timeout: f64) -> Option<String> {
    let herdr = herdr();
    let mut argv = vec![program.to_string_lossy().into_owned()];
    argv.extend(args.iter().map(|argument| argument.to_string()));
    let completed = herdr.run_command(&argv, timeout, false).ok()?;
    if completed.returncode != 0 {
        return None;
    }
    Some(completed.stdout)
}

/// Solo un `.exe` se ejecuta sin un shell en Windows.
#[cfg(windows)]
fn is_runnable(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_runnable(_path: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_arbol_de_procesos_incluye_al_hijo() {
        // El propio proceso de prueba y su padre forman un árbol real.
        let me = std::process::id();
        let (memory_mb, names) = process_tree(me);
        assert!(!names.is_empty());
        assert!(memory_mb <= bytes_to_mb(1_024 * 1_024 * 1_024));
        assert!(!descends_from(
            &System::new().expect("sistema"),
            Pid::from_u32(me),
            Pid::from_u32(u32::MAX)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn el_commit_libre_es_positivo_y_cabe_en_el_limite() {
        let (free, total) = commit::free_and_total_mb().expect("commit de Windows");
        assert!(free > 0, "commit libre: {free}");
        assert!(free <= total, "libre {free} > límite {total}");
    }

    #[test]
    fn un_cmd_no_es_ejecutable_directo_en_windows() {
        #[cfg(windows)]
        assert!(!is_runnable(Path::new("C:\\x\\mcode.cmd")));
        assert!(is_runnable(Path::new("C:\\x\\mcode.exe")));
    }
}
