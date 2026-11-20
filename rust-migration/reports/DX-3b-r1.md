# DX-3b r1 — Humo reproducible y el mismo servidor que hoy

Rama `work/DX-3`. Atiende las dos observaciones de la revisión de DX-3: el humo queda
comprometido como guion repetible y se demuestra, con la instalación real, que el
lanzamiento directo usa el mismo ejecutable y argumentos que el servidor de hoy. No se
tocó la app abierta de la persona ni su `whisper-server` (PID 1556); no se lanzaron
agentes reales. El brief `DX-3b.md` quedó sin seguimiento, como se pidió.

## Resultado
- Brief/tarea: DX-3b
- Estado: completado

## Cambios
- `scripts/tests/humo-whisper.ps1` (nuevo): humo repetible con `powershell -NoProfile
  -File`. Cuatro pasos (arranca y se mata sin aviso → el servidor muere; segundo arranque
  reutiliza uno sano; uno colgado se reemplaza; no queda ningún PID de la prueba), con
  `APPDATA`/`LOCALAPPDATA` temporales, el falso copiado como `whisper-server.exe`,
  procesos identificados por PID y limpieza en `finally` (también si falla a mitad).
- `rust/crates/ez-stt/examples/whisper_plan.rs` (nuevo): plan de solo lectura; imprime el
  ejecutable y los argumentos que lanzaría `ensure_whisper` sin lanzar nada. El puerto por
  omisión es 0 (marcador); acepta `-- <puerto>` para una comparación exacta.
- `rust/crates/ez-stt/src/server.rs`: `LaunchPlan`, `launch_plan` y `plan_for`; el
  lanzamiento real (`ensure_whisper_in`) usa el mismo `plan_for`, así que el ejemplo no
  puede divergir del arranque. Idioma e hilos quedan como constantes con su origen
  (`start-whisper.ps1`).
- `rust/crates/ez-stt/src/server/tests.rs`: prueba `the_plan_matches_the_installed_layout`
  con una carpeta temporal con la misma forma (exe en la raíz, `models/` con el modelo y el
  VAD) que fija los argumentos exactos.

## Decisiones
- `voice.json` no tiene claves de idioma ni hilos (el archivo real solo trae `stt_engine` y
  `stt_model`), así que el plan usa los valores por omisión de `start-whisper.ps1`: `-l es`,
  `-t 6`. No se añadieron claves nuevas a la configuración (fuera del alcance de este brief
  y `VoiceConfig` descarta claves desconocidas). El servidor de hoy corre justamente con
  `es`/`6`.
- El puerto por omisión del ejemplo es 0 porque `ensure_whisper` elige uno libre al lanzar;
  la comparación con el proceso real es «salvo el puerto». Con `-- 53584` la salida es
  idéntica a la línea real.
- El humo fuerza `WHISPER_URL` a un puerto cerrado para no confundir el 8088 de la persona
  con un servidor propio, y copia `espeak-ng` y `onnxruntime` de su instalación real (solo
  lectura) a la temporal para que la app no muera al cargar Kokoro.
- La limpieza solo cierra PIDs que el guion anotó (app, servidor del estado, manual y
  colgado); a la persona solo la lista, nunca la mata.

## Verificación
Ejecutado desde `rust/`, solo con los crates del brief:

```
cargo fmt -p ez-stt -p ez-fakes -- --check          → código 0, sin diffs
cargo clippy -p ez-stt -p ez-fakes --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.60s
cargo test -p ez-stt
    running 48 tests
    test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.09s
    test result: ok. 1 passed; 0 failed; ... (integraciones y doc-tests, todos en verde)
cargo test -p ez-fakes
    running 70 tests
    test result: ok. 70 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

### Humo desde el guion comprometido (release reconstruido tras el cambio)
```
powershell -NoProfile -File scripts\tests\humo-whisper.ps1

== 1. arranca, se mata sin aviso y el servidor muere ==
   app PID 4248, servidor PID 18420, puerto 65395, pico 12 MB
   OK: app 4248 y servidor 18420 muertos
== 2. un segundo arranque reutiliza uno sano ==
   OK: reutiliza el PID 22472 en el puerto 65398
   OK: el servidor manual no estaba en el Job y se cierra aparte
== 3. uno colgado se reemplaza ==
   OK: colgado 1940 cerrado; nuevo PID 13008 en 65414
   OK: el servidor nuevo muere con la app
== 4. no quedan procesos de la prueba ==
   OK: ninguno de los PIDs de la prueba sigue vivo; siguen los de la persona: 1556, 11628
HUMO: TODO OK
```

### Plan con la instalación y la configuración reales
```
cargo run -q -p ez-stt --example whisper_plan
ejecutable: C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\whisper-server.exe
argumentos: -m C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\models\ggml-large-v3-turbo-q8_0.bin --host 127.0.0.1 --port 0 -l es -t 6 --vad --vad-model C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\models\ggml-silero-v5.1.2.bin

cargo run -q -p ez-stt --example whisper_plan -- 53584
ejecutable: C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\whisper-server.exe
argumentos: -m C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\models\ggml-large-v3-turbo-q8_0.bin --host 127.0.0.1 --port 53584 -l es -t 6 --vad --vad-model C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\models\ggml-silero-v5.1.2.bin
```

Servidor real de la persona (PID 1556, leído sin tocarlo):
```
"C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\whisper-server.exe" -m C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\models\ggml-large-v3-turbo-q8_0.bin --host 127.0.0.1 --port 53584 -l es -t 6 --vad --vad-model C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\models\ggml-silero-v5.1.2.bin
```

Coinciden exactamente salvo el puerto (y las comillas del ejecutable, que el plan imprime
en su propia línea).

## Pendiente o riesgos
- El ejemplo no carga `.env` (la app sí), pero el `.env` del repositorio no define
  `WHISPER_ENGINE` ni `WHISPER_URL`; si en el futuro lo hiciera, el plan debería ejecutarse
  con ese entorno para reflejarlo. El motor y el modelo salen de `voice.json` en ambos.
- El humo necesita la instalación de Kokoro (`espeak-ng` y `onnxruntime`) para mantener la
  app viva en los cuatro pasos; si no está, avisa y el paso 1 falla con un mensaje claro.
- Sigue vivo el huérfano viejo PID 11628, anterior a DX-3; no se tocó. La corrección aplica
  a los arranques nuevos.
