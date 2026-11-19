# DX-3 — La app de voz deja `whisper-server` huérfanos (≈1,8 GB cada uno)

**Encargo de corrección.** Rama desde `eco/main`.

## Evidencia (19-nov, 14:00, el agente principal)
Había **7** `whisper-server.exe` vivos (12,5 GB de memoria privada) y una sola app de voz.
Cada arranque de la app (07:55, 08:50, 09:39, 09:46, 10:16, 13:49 —la caída— y 13:54)
lanzó su propio servidor (a través de un `powershell.exe` intermedio) y, al cerrarse o
caerse la app, el servidor quedó vivo. La memoria virtual libre llegó a 3,8 GB y el
guardián de memoria saltó. Uno de ellos (07:55) no se puede cerrar sin permisos de
administrador.

## Qué hacer
1. **Atar el servidor a la vida de la app**: lanzarlo dentro de un *Job Object* de Windows
   con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (crate `windows` del workspace), de modo que
   muera con la app aunque esta se caiga; y cerrarlo de forma ordenada al salir.
2. **Reutilizar si ya hay uno sano**: al arrancar, si existe un `whisper-server` de EzAgent
   que responde (puerto guardado en un archivo de estado propio, comprobado con su
   endpoint de salud), usarlo en vez de lanzar otro; si no responde, cerrarlo si es
   nuestro y lanzar uno nuevo.
3. Sin `powershell.exe` intermedio si no es imprescindible (lanzar el exe directo, sin
   shell); si lo es, que también quede en el Job.
4. `EzAgent.bat stop` cierra también los `whisper-server` de EzAgent.

## Pruebas
Con un servidor falso (`ez-fakes` tiene `fake-whisper`): la app de prueba arranca,
se mata sin aviso y el servidor muere con ella; un segundo arranque reutiliza uno sano;
uno colgado se reemplaza. Prueba de humo del binario release con `APPDATA` temporal:
arrancar, cerrar a la fuerza, comprobar que no queda `whisper-server`.

## Reglas
No toques la app abierta de la persona ni su `whisper-server`. Nada pasa de 1 GB.

## Criterios
fmt, clippy y test de los crates tocados en verde; informe `DX-3-r1.md` con la prueba de humo.

## Adenda del agente principal (antes de lanzar)
- **Archivos que se tocan**: `rust/crates/ez-stt/src/server.rs` (lo lanza hoy con
  `Command::new("powershell")` y un guion), un archivo nuevo `ez-stt/src/server/job.rs`,
  `ez-stt/src/server/tests.rs`, `ez-stt/Cargo.toml` (el crate `windows` del workspace con
  las features justas), `ez-fakes` si el servidor falso necesita algo, y
  `scripts/stop-voice.ps1` (lo llama `EzAgent.bat stop`). En paralelo DX-1d toca `ez-app` y
  `ez-core`: **no los toques**; si hace falta cambiar `ez-app`, detente y dilo en el informe.
- **Excepción a `_COMMON-ECO`**: el `unsafe` de las llamadas al Job Object
  (`CreateJobObjectW`, `SetInformationJobObject`, `AssignProcessToJobObject`) solo en
  `server/job.rs`, cada bloque con `// SAFETY:`. Los procesos hijos del `powershell`
  intermedio heredan el Job: si lo conservas, comprueba que el `whisper-server` real (el
  nieto) también muere.
- La prueba de humo usa el **servidor falso** (`fake-whisper`) configurado como servidor de
  la app con `APPDATA` temporal; nunca el `whisper-server` real ni el modelo (pasaría de
  1 GB). Identifica los procesos de tu prueba por PID, nunca por nombre: la persona tiene su
  propio `whisper-server` vivo y no se toca.
- Rama `work/DX-3` desde `eco/main`. Compila en primer plano y espera el resultado.
