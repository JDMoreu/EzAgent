# Grabaciones para el banco de latencia

El banco necesita **20 frases grabadas por ti** (no se inventa audio). Las
frases están en `phrases.json`; cada WAV se llama `<id>.wav` y se guarda en esta
carpeta. Los `.wav` no se versionan (`.gitignore`).

## Formato
WAV **mono, 16 bits, 16 kHz**. Con `ffmpeg` se convierte cualquier grabación:

```powershell
ffmpeg -i grabacion.m4a -ac 1 -ar 16000 -sample_fmt s16 c01.wav
```

## Cómo grabar
- Mismo micrófono y misma habitación que usas con EzAgent.
- Empieza a hablar a ~0,5 s del inicio y termina la grabación a ~0,3 s después de
  la última palabra (el banco añade el silencio final).
- Grupo `pausa`: haz la pausa que indica la nota **a mitad de frase**; sirve para
  comprobar que el VAD no corta el turno.
- Habla a tu ritmo normal. No leas rápido ni despacio a propósito.

## Ejecución
```powershell
uv run python -m bench.latency --runs 3        # componentes reales
uv run python -m bench.latency --stt fake --llm fake --tts null --synthetic   # prueba sin audio ni red
```
El informe JSON queda en `bench/out/` y los números se copian a
`docs/roadmap/metricas.md`.

## Qué mide
- `vad_wait_ms`: silencio que espera el VAD (tiempo de audio, determinista).
- `stt_ready_ms`, `first_token_ms`, `first_sentence_ms`, `first_sound_ms`: tiempo
  real entre fases.
- `ttfa_ms`: de que dejas de hablar al primer sonido. En el banco, «primer
  sonido» es el final de la **síntesis** de la primera frase; no incluye abrir el
  stream de audio.
- `cut`: el VAD cerró el turno antes del final de la frase.
- `wer`: error de palabras de la transcripción contra el texto de `phrases.json`.
