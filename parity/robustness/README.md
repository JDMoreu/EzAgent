# Robustez (F7-05)

Escenarios de fallo y la prueba de fondo de 2 h. Cada caso corre con **los dos
motores** —el `VoiceApp` de Python (`parity/replay/run_py.py`) y el de Rust
(`ezagent-replay`)— y se compara la conducta observable: frases dichas,
mensajes del chat, transcripciones y estado final. No se compara el texto que
solo depende del cliente HTTP (httpx en Python, reqwest en Rust): esos detalles
se normalizan.

## Casos

| Caso | Qué rompe |
|---|---|
| R01 | whisper muere con el turno en vuelo; el siguiente turno ya no conecta |
| R02 | whisper responde 500 y después vuelve a transcribir |
| R03 | whisper cuelga más que `stt_timeout_secs` y después vuelve |
| R04 | el LLM corta el SSE a mitad del streaming y después responde entero |
| R05 | el LLM responde 401 y después un turno bueno |
| R06 | el LLM responde 429 con `Retry-After` y después un turno bueno |
| R07 | el LLM responde 429 sin `Retry-After` y después un turno bueno |
| R08 | el micrófono se pierde a mitad de llamada |

Los pasos nuevos que usan los casos:

- `whisper.faults`: guion de fallos de `fake-whisper`, uno por petición.
  `{"status": 500, "error": "..."}` responde un error; `{"drop": true}` mata el
  proceso con la petición en vuelo; `delay_ms` espera antes.
- `mic_fail: "motivo"`: el doble de captura pierde el dispositivo; la app
  recibe el mismo aviso que daría `MicCapture` al fallar.

## Uso

```powershell
# compara los ocho casos (necesita fake-* y ezagent-replay compilados)
cargo build -p ez-fakes --bins
cargo build -p ez-app --features replay --bin ezagent-replay
python parity/robustness/run_robustness.py compare

# un caso suelto
python parity/robustness/run_robustness.py compare --case R08

# prueba de 2 h (en segundo plano); muestrea RSS, hilos y handles cada minuto
python parity/robustness/run_robustness.py soak --minutes 120 --work C:\temp\soak
```

`--work` guarda las trazas, el JSON de cada caso y, en el soak, `soak.json` con
las muestras y el veredicto (RSS estable ±5 % tras el calentamiento, hilos y
handles sin crecer).
