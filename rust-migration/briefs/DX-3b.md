# DX-3b — Corrección de la revisión de DX-3: humo reproducible y el mismo servidor que hoy

Revisión del agente principal sobre `work/DX-3` (0051985 + 9a9e4cb). Bien: Job Object con
`KILL_ON_JOB_CLOSE`, `unsafe` solo en `server/job.rs` con `// SAFETY:`, estado propio con
comprobación del nombre de imagen antes de matar, reutilización y reemplazo, humo del
binario release con el servidor falso y los procesos de la persona intactos. Faltan dos
cosas antes de instalarlo en la app de la persona:

1. **El humo no está en el repositorio.** El informe pega su salida, pero el guion que la
   produjo no se comprometió: nadie puede repetirlo (el revisor tampoco).
2. **Lanzar directo duplica la lógica de `start-whisper.ps1`.** Si la selección difiere,
   la persona cambia de motor, modelo o idioma sin enterarse. Hoy su servidor (lanzado por
   el guion) corre exactamente así:

   ```
   "C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\whisper-server.exe"
     -m C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\models\ggml-large-v3-turbo-q8_0.bin
     --host 127.0.0.1 --port 53584 -l es -t 6 --vad
     --vad-model C:\Users\JDMoreu\AppData\Local\EzAgent\whisper\models\ggml-silero-v5.1.2.bin
   ```
   (guion: `start-whisper.ps1 -Port 53584 -Engine auto -WhisperHome …\whisper -Model …`).

## Cambios
1. Compromete el humo como guion repetible (`scripts/tests/humo-whisper.ps1`, ejecutable
   con `powershell -NoProfile -File`), con sus cuatro pasos, `APPDATA`/`LOCALAPPDATA`
   temporales, el falso como `whisper-server.exe`, procesos identificados por PID y
   limpieza al final (también si falla a mitad).
2. Un ejemplo de solo lectura `cargo run -p ez-stt --example whisper_plan` que, con la
   instalación y la configuración **reales** (`voice.json`: motor, modelo, idioma, hilos;
   nada más), imprime el ejecutable y los argumentos que lanzaría `ensure_whisper`, **sin
   lanzar nada**. Pega su salida en el informe junto a la línea de arriba: deben coincidir
   salvo el puerto. Si difieren, corrige la selección hasta que coincidan.
3. Una prueba de unidad que fije esa misma correspondencia con una carpeta de whisper
   temporal con la misma forma (exe en la raíz, `models/` con el modelo y el VAD).

## Entrega
fmt, clippy y test de `ez-stt` y `ez-fakes` en verde; el humo ejecutado **desde el guion
comprometido**, salida literal; la salida de `whisper_plan`. No toques la app ni el
`whisper-server` de la persona (ahora PID 1556). Commit en `work/DX-3`, informe
`rust-migration/reports/DX-3b-r1.md`. Compila en primer plano.
