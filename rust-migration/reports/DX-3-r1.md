# DX-3 r1 — La app de voz deja de dejar `whisper-server` huérfanos

Rama `work/DX-3` desde `eco/main`. El servidor de whisper se lanza ahora directo (sin
`powershell` intermedio) dentro de un Job Object con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
se reutiliza si el de un arranque anterior sigue sano y se reemplaza si quedó colgado.
No se tocó la app de voz abierta de la persona, ni su `whisper-server`, ni Herdr; no se
lanzaron agentes reales.

## Resultado
- Brief/tarea: DX-3
- Estado: completado

## Cambios
- `rust/crates/ez-stt/src/server.rs`: resuelve y lanza `whisper-server.exe` directo (misma
  selección de motor, modelo por omisión y VAD que `scripts/start-whisper.ps1`), lo asigna
  al Job, guarda `{pid, port, url}` en `%APPDATA%\EzAgent\whisper-server.json` (escritura
  atómica) y al arrancar reutiliza/cierra el del estado. El sondeo de salud usa `/health`
  con la conexión TCP como respaldo para binarios viejos. `ensure_whisper` mantiene su
  firma; el núcleo inyectable (`ensure_whisper_in`) permite probar sin tocar la
  instalación real.
- `rust/crates/ez-stt/src/server/job.rs` (nuevo): Job Object del proceso, `assign`,
  `process_image_name` y `terminate`. Es el único archivo con `unsafe` (excepción de la
  adenda), cada bloque con `// SAFETY:`.
- `rust/crates/ez-stt/src/server/tests.rs`: pruebas de estado (ida y vuelta, reutilización
  de un servidor sano, cierre de uno colgado, PID ajeno intacto), resolución de
  ejecutable/modelo, argumentos y salud. Todas usan temporales y una sonda falsa; ninguna
  resuelve la carpeta real de whisper de la persona.
- `rust/crates/ez-stt/Cargo.toml`: `windows` del workspace con
  `Win32_Foundation`, `Win32_Security`, `Win32_System_JobObjects` y
  `Win32_System_Threading`; `tiny_http` como dependencia de pruebas.
- `rust/crates/ez-fakes/src/fake_whisper.rs`: tolera las banderas de `whisper-server`
  (`-m`, `--host`, `-l`, `-t`, `-ng`, `--vad`, `--vad-model`…) para poder ocupar su lugar
  en la prueba de humo; los argumentos desconocidos siguen siendo error.
- `scripts/stop-voice.ps1`: además del patrón viejo de 8088, cierra el PID que dejó la app
  en `%APPDATA%\EzAgent\whisper-server.json` (comprobando que el proceso siga siendo
  `whisper-server`) y retira el estado.
- `rust/Cargo.lock`: solo el efecto de compilar (features de `windows`).

## Decisiones
- `// DECISION: se lanza el ejecutable directo, sin «powershell» intermedio; el script
  queda solo para arrancarlo a mano.` El proceso lanzado es el que queda en el Job y el
  que se guarda; así no hay nietos que comprobar. La selección replica la del script
  (orden vulkan/CPU, modelos por omisión y VAD junto al modelo).
- `// DECISION: app.py usa log() propio (sin niveles)…` (se conserva del porte).
- `// DECISION: un estado ilegible también es nuestro: se descarta.`
- Estado en `%APPDATA%\EzAgent\whisper-server.json` (archivo propio de la app; no se
  escribe en la configuración de la persona). Se borra cuando el PID ya no es un
  `whisper-server.exe`, para no matar nunca un proceso ajeno reutilizado por Windows.
- El Job se crea una vez por proceso y lo cierra el sistema al salir o al caer la app; no
  hace falta cambiar `ez-app` (sus `Option<Child>` siguen igual).
- Si `AssignProcessToJobObject` falla (por ejemplo, la app ya vive dentro de un Job
  restrictivo), se avisa por `tracing` y el servidor sigue funcionando sin protección.
- `/health` con respaldo TCP: un whisper.cpp viejo sin esa ruta se considera sano si el
  puerto acepta conexiones, para no matar un servidor que sí funciona.

## Verificación
Ejecutado desde `rust/`, solo con los crates del brief:

```
cargo fmt -p ez-stt -p ez-fakes -- --check          → código 0, sin diffs
cargo clippy -p ez-stt -p ez-fakes --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 00s
cargo test -p ez-stt
    running 47 tests
    test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.07s
    test result: ok. 1 passed; 0 failed; ... (integraciones y doc-tests, todos en verde)
cargo test -p ez-fakes
    running 70 tests
    test result: ok. 70 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
```

### Prueba de humo real (binario release, `fake-whisper`, `APPDATA`/`LOCALAPPDATA` temporales)
`ezagent-voice.exe` release y `fake-whisper.exe` copiado como `whisper-server.exe` en una
carpeta de whisper temporal; `WHISPER_URL` a un puerto cerrado para no usar el 8088 de la
persona. Salida literal:

```
== 1. arranca, se mata sin aviso y el servidor muere ==
   app PID 8888, servidor PID 19036, puerto 56535, pico 13 MB
   OK: app 8888 y servidor 19036 muertos
== 2. un segundo arranque reutiliza uno sano ==
   OK: reutiliza el PID 20420 en el puerto 56540
   OK: el servidor manual no estaba en el Job y se cierra aparte
== 3. uno colgado se reemplaza ==
   OK: colgado 13332 cerrado; nuevo PID 16444 en 56559
   OK: el servidor nuevo muere con la app
== 4. no quedan procesos de la prueba ==
   OK: solo siguen los de la persona: 11628, 18960
PRUEBA DE HUMO: TODO OK
```

El paso 1 usa `Stop-Process -Force` (TerminateProcess, sin aviso); el servidor muere al
cerrarse el Job. El paso 2 arranca un `fake-whisper` manual, escribe el estado con su PID
y comprueba que la app no crea ningún hijo `whisper-server`. El paso 3 pone en el estado un
`PING.EXE` copiado como `whisper-server.exe` (nunca responde); la app lo cierra por PID y
lanza el falso nuevo. En todo momento los únicos `whisper-server` vivos al final son los
dos que ya tenía la persona (11628 y 18960), que no se tocaron.

También se ejecutó `scripts/stop-voice.ps1` contra un estado temporal y un falso propio,
sustituyendo `Get-CimInstance` por una función vacía para no mirar procesos reales:

```
Deteniendo whisper-server de EzAgent (PID 16364)...
STOP-VOICE: TODO OK (servidor 16364 cerrado y estado retirado)
```

## Pendiente o riesgos
- Los dos servidores huérfanos que ya existían (PIDs 11628 y 18960, uno de ellos sin
  permisos para leer su línea de comandos) siguen vivos: la corrección aplica a los
  arranques nuevos. Se pueden cerrar a mano una vez o al reiniciar; no se tocaron.
- Si la app se reutiliza un servidor de otra instancia viva, `restart_whisper_async` de
  `ez-app` no puede reiniciarlo (no tiene el `Child`): el cambio de modelo se aplica en el
  siguiente arranque. No se tocó `ez-app` por el alcance del brief.
- La selección de motor/modelo/VAD está duplicada respecto de `start-whisper.ps1`; si el
  script cambia, hay que sincronizar `server.rs`. Se eligió así para lanzar el ejecutable
  directo, como pide el brief.
- Si Windows niega `AssignProcessToJobObject` (Job padre restrictivo), queda el aviso en
  el registro y el servidor no muere con la app; el estado permite reutilizarlo o
  cerrarlo en el siguiente arranque.
