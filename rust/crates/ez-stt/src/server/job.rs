//! Job Object de Windows: el `whisper-server` muere con la app (DX-3).
//!
//! El Job se crea una vez por proceso con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`:
//! cuando se cierra su último handle (salida ordenada, `TerminateProcess` de la
//! app o una caída), Windows mata a todos los procesos que contiene. El
//! servidor se asigna justo después de lanzarlo.
//!
//! Este es el único archivo donde se permite `unsafe` (adenda de DX-3); cada
//! bloque lleva su `// SAFETY:`.

use std::ffi::c_void;
use std::path::Path;
use std::process::Child;
use std::sync::OnceLock;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
};

/// Job del proceso; se crea la primera vez que hace falta.
struct Job(HANDLE);

// SAFETY: el handle del Job vale para todo el proceso y las llamadas de Job
// Objects son seguras entre hilos.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: el handle lo devolvió `CreateJobObjectW` y solo se cierra aquí.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

impl Job {
    fn create() -> Option<Self> {
        // SAFETY: sin atributos de seguridad ni nombre; el handle se comprueba.
        let handle = unsafe { CreateJobObjectW(None, PWSTR::null()) }.ok()?;
        let job = Self(handle);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `info` es la estructura de la clase pedida y se pasa su tamaño;
        // el handle del Job es válido.
        let configured = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        // Si no se pudo configurar, el `Drop` cierra el handle y se sigue sin Job.
        configured.ok().map(|()| job)
    }
}

fn process_job() -> Option<&'static Job> {
    static JOB: OnceLock<Option<Job>> = OnceLock::new();
    JOB.get_or_init(Job::create).as_ref()
}

/// Mete `child` en el Job que mata al cerrarse la app.
///
/// Devuelve `false` si Windows no deja crear el Job o asignar el proceso; el
/// servidor sigue funcionando, solo que sin esa protección.
pub fn assign(child: &Child) -> bool {
    let Some(job) = process_job() else {
        return false;
    };
    // SAFETY: se abre el hijo que acabamos de lanzar con los permisos que exige
    // `AssignProcessToJobObject`; el handle se cierra abajo.
    let Ok(process) =
        (unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, child.id()) })
    else {
        return false;
    };
    // SAFETY: ambos handles son válidos y el proceso es hijo de este proceso.
    let assigned = unsafe { AssignProcessToJobObject(job.0, process) }.is_ok();
    // SAFETY: el handle de proceso se abrió aquí y ya no se usa.
    let _ = unsafe { CloseHandle(process) };
    assigned
}

/// Nombre del ejecutable de un PID (sin la carpeta), o `None` si no existe.
pub fn process_image_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    // SAFETY: se abre el proceso solo para consultarlo; el handle se cierra abajo.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    // SAFETY: el buffer y su tamaño son válidos; `size` recibe la longitud escrita.
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    // SAFETY: el handle de proceso se abrió aquí y ya no se usa.
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// Mata el proceso `pid` (el que la app lanzó antes y quedó colgado).
pub fn terminate(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: se abre el proceso con permiso de terminarlo; el handle se cierra abajo.
    let Ok(process) = (unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }) else {
        return false;
    };
    // SAFETY: el handle se abrió con `PROCESS_TERMINATE`; el código de salida es
    // el que usa `TerminateProcess` en el resto del código del proyecto.
    let terminated = unsafe { TerminateProcess(process, 1) }.is_ok();
    // SAFETY: el handle de proceso se abrió aquí y ya no se usa.
    let _ = unsafe { CloseHandle(process) };
    terminated
}
