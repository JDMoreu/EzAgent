# Métricas

Se rellena desde E1. Cada fila indica la máquina, el commit, el modelo y el
comando con que se obtuvo. No se publican cifras que no salgan de aquí.

## Máquina de referencia

| Campo | Valor |
|---|---|
| SO | Windows 11 Home 10.0.26300 |
| CPU / GPU | <completar en E1> |
| Audio | <auriculares / micrófono> |
| whisper.cpp | <versión, modelo, backend> |
| Kokoro | <variante> |

## Latencia (bench/latency, 20 frases)

| Fecha | Commit | Etapa | LLM | vad_wait P50 | stt P50 | first_token P50 | Fin de voz → primer sonido P50 / P95 | Turnos cortados |
|---|---|---|---|---|---|---|---|---|
| | | línea base (E1) | deepseek-v4.1-flash | | | | | |

## Set dorado (evals/intents)

| Fecha | Commit | Modelo | Casos | Precisión total | es | en | TTFT P50 | Coste / 1 000 turnos |
|---|---|---|---|---|---|---|---|---|
| | | | | | | | | |

## Notch

| Fecha | Commit | UI | CPU reposo | CPU hablando | fps P5 hablando | RAM UI |
|---|---|---|---|---|---|---|
| | | | | | | |

## Observaciones preliminares (8 nov 2026, rama E0)

Dos turnos a mano con el notch real, sin el banco de E1. **No son línea base**:
muestra mínima y sin protocolo. Sirven solo para orientar E2.

| Turno | vad_wait | stt | first_token | frase→primer sonido | fin de voz → primer sonido |
|---|---|---|---|---|---|
| «¿Me escuchas?» | 1 281 ms | 141 ms | 1 734 ms | 2 484 ms | 5 687 ms |
| «Quién eres» | 1 281 ms | 110 ms | 1 531 ms | 4 031 ms | 6 953 ms |

Lectura: STT (≈ 0,1 s) no es el cuello de botella. Pesan el silencio fijo
(1,3 s), el primer token del LLM (≈ 1,5 s) y, sobre todo, el tramo entre «primera
frase lista» y «primer sonido» (2,5–4 s). Objetivo: ≤ 900 ms de mediana.
