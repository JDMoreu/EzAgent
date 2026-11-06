# Latencia de voz · medición y ajustes

Fecha: 6 nov 2026. Alcance: mensajero Python del notch (`src/ezagent/voice`).
El objetivo es medir el turno completo y reducir la espera sin cambiar de modelo.

## Qué se mide

| Marca | Origen | Significado |
| --- | --- | --- |
| `speech_end` | VAD/segmenter | último bloque con voz antes del silencio |
| `vad_end` | callback de utterance | el VAD cierra el turno y entrega el PCM |
| `stt_ready` | worker de STT | whisper devuelve la transcripción |
| `first_token` | streaming del LLM | primer delta de texto útil |
| `first_audio` | worker de TTS | empieza la síntesis de la primera cláusula |
| `first_sound` | reproducción | primer bloque escrito al dispositivo de salida |
| `turn_end` | `answer_done` | termina el turno |

El mensajero Python imprime una línea por turno:

```
[ezagent-voice] Latencia: vad_wait_ms=... stt_ready_ms=... first_token_ms=... first_sound_ms=... total_ms=...
```

Cada diez turnos añade media y P95. Solo se registran turnos que llegan a `answer_done`; los turnos cortados o superados por otra frase se cuentan aparte en el protocolo de validación.

## Ajustes aplicados

- **Intenciones locales:** mute, desmute, cerrar escucha, repetir, saludo y
  «¿qué agentes hay?» se resuelven en `_fast_command` sin LLM.
- **Streaming:** el mensajero emite la primera cláusula corta (coma, dos
  puntos o 40 caracteres) y después frases completas.
- **Calentamiento:** Kokoro se sintetiza al arrancar y el mensajero hace además
  una petición de silencio a whisper para cargar el modelo.
- **STT:** el mensajero fija `language=es`, `temperature=0` y `response_format=json`.
  El modelo q8_0, beam 1 y sin timestamps quedan pendientes: q8_0 no está
  instalado y `beam_size=1` puede ser más lento que el greedy actual; se
  medirá antes de cambiarlo.
- **Audio:** el mensajero ya escribe bloques de 20 ms (480 frames) en la salida.
- **Eco/barge-in:** se mantiene el umbral de barge-in y el reset del VAD al
  terminar el TTS; no se añadió cancelación de eco por hardware.

## Protocolo de validación

1. Grabar las mismas 20 frases en español, con pausas a media frase y comandos
   de Herdr, en WAV mono 16 kHz. Guardar una versión «antes» y otra «después».
2. Ejecutar cada frase por el mismo dispositivo y la misma configuración.
3. Registrar `total_ms` (fin de voz→primer audio), si la transcripción fue
   correcta, si el turno se cortó y si el LLM llamó a la herramienta correcta.
4. Calcular media y P95 por frase y por grupo (pausas / comandos).
5. Revertir un cambio aislado si empeora la transcripción o corta turnos,
   aunque mejore la latencia.

Los tiempos de referencia que publica Sonnet son una estimación
(0,8–1,2 s fin de voz→primer audio); no se presentan como medición de esta
máquina hasta completar el protocolo.

## Pendiente / siguientes pasos

- Modelo `ggml-large-v3-turbo-q8_0`, `beam_size=1` y `no_timestamps` cuando el
  fichero esté instalado y el servidor acepte esos parámetros.
- Acuse pregrabado si el LLM tarda más de un umbral; hoy la isla pasa a PIENSO
  al instante.
- Evaluar Supertonic/parakeet.cpp solo con el mismo protocolo de 20 frases.
