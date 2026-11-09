# EzAgent

**Capa de voz local para agentes de código.** Hablas; EzAgent entiende la
intención, delega la tarea en Claude Code, OpenCode, Codex u otro CLI (directamente
o a través de Herdr), vigila el progreso y te avisa por voz cuando termina o se
bloquea. No es un asistente general: el trabajo real lo hacen los agentes
delegados.

La interfaz es una **barra flotante** sin texto: una cápsula (o un círculo) con
un gradiente vivo, **Marea**, que fluye con tu voz y con la del asistente, y
una ventana de ajustes centrada que se abre desde ☰. No hay app de escritorio, dashboard
ni panel de llamada.

```
Micrófono
   │
   ▼
VAD por energía ──► Smart Turn (¿terminaste la frase?) ──► whisper.cpp (es)
                                                                │
                         atajos locales (mute, cierra, gracias) ◄┤
                                                                ▼
              LLM en streaming + herramientas (OpenAI o Anthropic)
              ├─ acuse inmediato: «Va, lo reviso.»
              ├─ archivos · web · memoria · skills
              └─ Herdr: OpenCode · Claude Code · Codex · mcode
                                                                │
                                                                ▼
            Kokoro: sintetiza la frase siguiente mientras suena la actual
                                                                │
                                                                ▼
                                                             Altavoz
```

| Capa | Motor | Dónde corre |
|---|---|---|
| VAD / segmentación | `Segmenter` con pre-roll e histéresis | Local |
| Fin de turno | Smart Turn v3.2 (ONNX, 8 MB) | Local, CPU |
| STT | whisper.cpp + `large-v3-turbo` | Local, GPU AMD vía Vulkan |
| LLM | Proveedor configurable, compatible con OpenAI o con Anthropic | Nube |
| TTS | Kokoro v1.0, voces españolas, en cola | Local, CPU (ONNX) |

Recorrido completo de un turno, hilos y archivos:
[`docs/architecture.md`](docs/architecture.md).

**Latencia típica** (medida con OpenCode Go): fin de turno en ~0,4 s cuando la
frase está completa (antes, 1,3 s fijos), transcripción en ~0,2 s y primer
audio a los ~3,6 s de media; el resto es la espera del primer token del LLM.

**Pipecat no se usa.** El pipeline es propio (VAD, whisper.cpp, Kokoro y el
LLM); la carpeta `~/.cache/pipecat/kokoro-onnx` es una caché antigua que solo
se lee para migrar los modelos Kokoro, que se copian una vez a
`%LOCALAPPDATA%\EzAgent\models\kokoro`.

## Arranque

Requisitos: Windows 11, `uv` y una clave del proveedor LLM elegido. Los modelos
y binarios se descargan una sola vez, con verificación SHA256, a
`%LOCALAPPDATA%\EzAgent` (whisper.cpp, modelo Whisper, Kokoro y Smart Turn):

```powershell
uv run python -m ezagent.core.downloads --list   # qué falta
uv run python -m ezagent.core.downloads --all    # descarga lo necesario
```

La descarga automática instala whisper.cpp en **CPU**, porque el proyecto
oficial no publica binarios de Windows con Vulkan. Si tienes uno propio, copia
su `whisper-server.exe` y DLL a `%LOCALAPPDATA%\EzAgent\whisper` y se usará
antes que el de CPU. Otra carpeta: `EZAGENT_WHISPER_HOME` o `whisper_home` en
`voice.json`.

```powershell
git clone <url-del-repositorio> EzAgent
cd EzAgent
uv sync
# Opcional: dependencias de las skills de Word/PDF/Excel
uv sync --extra office
copy .env.example .env
# edita .env: elige LLM_PROVIDER y pon su clave

# Terminal 1: STT
powershell -ExecutionPolicy Bypass -File scripts\start-whisper.ps1

# Terminal 2: mensajero + barra
powershell -ExecutionPolicy Bypass -File scripts\start-voice.ps1
```

O doble clic en `EzAgent.bat` y elige **Iniciar mensajero de voz**. La app
arranca escuchando; `Ctrl+Alt+Space` cuelga o retoma.

## La barra (interfaz por defecto)

La interfaz es **una barra sin texto**: una cápsula (o un círculo) anclada abajo
al centro, sobre la barra de tareas. **El contorno es fijo y nítido; lo que se
mueve es el interior**: **Marea**, la identidad visual de EzAgent, un gradiente
orgánico como tintas que se mezclan en el agua (verde azulado, aguamarina y
menta, con filamentos melocotón). No es un orbe de nubes ni un brillo en el
borde: la marca es el propio líquido en movimiento.

| Estado | Cómo se mueve |
|---|---|
| En espera | Fluye y se pliega despacio |
| Te escucha | Tu voz lo acelera y lo empuja hacia arriba; entra una luz desde abajo |
| Habla EzAgent | Lo recorren pulsos de luz al ritmo de su voz |
| Piensa | Gira despacio, como un remolino (tonos índigo y cian) |
| Pregunta · error · pausa | Ámbar · rojo · gris lento |

Los colores siguen al estado y al tema, claro u oscuro; con movimiento reducido
el fluido queda quieto. El código vive en `src/ezagent/voice/notch/flow.py`.

Los botones solo aparecen al acercar el puntero: la forma crece un poco y ☰,
🎤 y ✕ nacen desde detrás de ella, escalonados y con un rebote suave. La barra
pinta a 60 fps fijos mientras algo se mueve. **✕ es dinámico:** con la llamada
en marcha cuelga; al colgar, el micrófono se pliega y ✕ se convierte en el botón
verde azulado de **llamar** (ondas de voz), que vuelve a abrir la conversación.
Una confirmación oculta ☰, convierte 🎤 en ✓ y deja ✕ como «no» (la pregunta se
dice por voz). Se arrastra por la forma y recuerda su posición.

**El micrófono tiene tres estados:**

| Estado | Cómo se ve | Qué escucha | Cómo se sale |
|---|---|---|---|
| Escuchando | 🎤 normal | Todo | — |
| En pausa | 🎤 tachado | Solo la frase de reactivación | Clic en 🎤 o decir «EzAgent, escúchame» |
| Colgado | Sin 🎤; ✕ pasa a «llamar» | Nada | Clic en «llamar» o `Ctrl+Alt+Space` |

«Cierra la escucha», «pausa» o «silencia el micro» dejan la escucha **en pausa**;
«adiós» o «cuelga» cuelgan. En pausa el micrófono sigue abierto en local, pero
solo se transcriben frases cortas (≤ 3 s) y solo cuentan si la frase entera es la
de reactivación: tiene que nombrar a EzAgent y decir el verbo («EzAgent,
escúchame», «EzAgent, vuelve», «EzAgent, abre la escucha»). Lo que no coincide se
descarta sin llegar al modelo ni al log. Si apagas «Reactivar por voz», la pausa
es un silencio total y solo el botón la quita. El modelo ya no puede silenciar el
micrófono: si lo pide, la escucha queda en pausa (siempre reversible).

**Ajustes ☰:** se abre una **ventana centrada** con las secciones a la izquierda
y la sección elegida a la derecha (blanco con un toque de menta en claro, noche
verde azulada en oscuro). «–» la oculta; la
barra sigue a la vista. Arriba están **Apariencia** (forma, tema y movimiento) y
**Escucha** (qué hace «cierra», reactivar por voz y la frase); se aplican y se
guardan al instante. En **Conversación** están «Repetir lo último» y «Nueva
conversación», que pide un segundo clic («¿Seguro?») antes de borrar.

**Ratón fluido con el menú abierto.** Antes, al abrir la tarjeta el puntero de todo
el sistema se sentía con retardo y al cerrarla volvía a ir bien. La causa era un
gancho global de ratón (`WH_MOUSE_LL`) instalado solo para la rueda: Windows lo
invoca en cada movimiento del puntero y respondía un hilo de Python que además
estaba dibujando la tarjeta (23–56 ms por cuadro). Ahora la tarjeta recibe la
rueda directamente de Windows (ajuste «Desplazar ventanas inactivas al pasar el
ratón», el de fábrica en Windows 10 y 11) y **no instala el gancho**; solo lo
conserva como respaldo si ese ajuste está desactivado o no se puede leer. Además,
el cuerpo de la tarjeta se dibuja una sola vez y se reutiliza (el cuadro con el
menú quieto baja ~40 %). Si algún día falla la rueda dentro de la tarjeta, mira
ese ajuste en Configuración → Bluetooth y dispositivos → Mouse.

**Círculo o píldora:** cámbialo en Ajustes → Apariencia, haz doble clic sobre la
forma de color o usa «Cambiar forma» en el menú de la bandeja.

Se configura en `voice.json`:

| Clave | Valores | Por defecto |
|---|---|---|
| `ui_layout` | `pill` (cápsula) · `circle` (círculo sobre la fila de botones) | `pill` |
| `theme` | `auto` (sigue a Windows) · `light` · `dark` · `custom` (tus colores) | `auto` |
| `theme_primary` · `theme_secondary` · `theme_accent` · `theme_background` | colores `#RRGGBB` del tema personalizado | paleta Marea |
| `ui_motion` | `system` (sigue a Windows) · `full` · `reduced` (solo fundidos de 120 ms) | `system` |
| `close_action` | `pause` (pausa la escucha) · `hangup` (cuelga) | `pause` |
| `voice_wake` | `true` · `false` (la pausa es silencio total) | `true` |
| `wake_phrase` | frase que reabre la escucha; si no nombra a EzAgent, se le antepone | `EzAgent, escúchame` |
| `barge_in` | `true` · `false` (nadie interrumpe al asistente mientras habla) | `true` |
| `barge_in_rms` | nivel que tu voz debe superar para interrumpir (0,008 ≈ −42 dB); también en Ajustes → «Umbral de interrupción», y se aplica en vivo | `0.008` |
| `barge_in_secs` | segundos de voz por encima de ese nivel para interrumpir | `0.25` |

**Interrumpir al asistente.** Mientras habla, solo lo cortas si tu voz supera
`barge_in_rms` durante `barge_in_secs` seguidos (con pausas de hasta 90 ms entre
sílabas). Un clic, un golpe de teclado o un soplido duran menos y no lo cortan.
Si aun así el ruido de fondo es sostenido (un ventilador, música), sube el umbral
o la duración; si te cuesta interrumpir, bájalos.

El icono (app y bandeja) es un orbe de Marea; se regenera con
`uv run python scripts/make_icons.py`.

## Bandeja del sistema

Mientras EzAgent está abierto, aparece un icono en la bandeja de Windows con el
menú contextual:

- **Configurar:** muestra la barra si estaba oculta y abre Ajustes.
- **Cambiar forma (círculo / píldora):** alterna la forma de la barra.
- **Reiniciar EzAgent:** cierra esta instancia y lanza una nueva con la misma
  configuración y argumentos.
- **Cerrar:** cierra la aplicación.

Windows puede colocar el icono nuevo en el desbordamiento de la bandeja (la
flecha `^`) la primera vez; se puede arrastrar a la zona visible. El tooltip
refleja el estado: en espera, escuchando, en conversación o micrófono
silenciado.

## El menú es Ajustes

La ventana de ajustes (☰, o `Ctrl+Alt+O`) se abre **en el centro de la
pantalla**. A la izquierda están las secciones; a la derecha, los controles de
la sección elegida, con todo el alto de la ventana, así casi nunca hace falta
desplazarse. «–» la oculta y la barra sigue a la vista. Si una sección no cabe,
la rueda desplaza su contenido entre la cabecera y el pie. Los campos de texto
se editan en línea: al pulsarlos, la ventana pide el teclado y lo
devuelve al confirmar con `Enter`, cancelar con `Esc` o pulsar fuera. `Ctrl+V`
pega desde el portapapeles.

- **APARIENCIA:** forma (píldora o círculo), tema (automático, claro, oscuro o
  **personalizado**) y movimiento (según el sistema, completo o reducido). Se
  aplica y se guarda al instante.
- **COLORES:** las paletas de Claro, Oscuro y Personalizado lado a lado (un clic
  en una la elige), **Partir de** (paletas base: Marea, Atardecer, Bosque, Lavanda
  y Medianoche) y los cuatro colores del gradiente con su muestra: **principal,
  secundario, acento y fondo** (`#RRGGBB`). Cambiar un color activa el tema
  personalizado; las superficies siguen a Windows (claro u oscuro) y los avisos
  conservan su color (ámbar al preguntar, rojo en error).
- **ESCUCHA:** qué hace «cierra» (pausa o cuelga), reactivar por voz y la frase de
  reactivación. También se aplica y se guarda al instante.
- **MICRÓFONO:** dispositivo, **medidor de entrada con nivel en vivo y umbral
  arrastrable** y umbral de ruido.
- **FIN DE TURNO:** silencio máximo, **cierre inteligente** (Smart Turn) y
  **certeza para cerrar** (súbela si te corta antes de tiempo), antelación de
  escucha, duración mínima y frase máxima.
- **CONVERSACIÓN:** interrupción activa y su umbral, confirmación opcional,
  prueba de voz, «Repetir lo último» y «Nueva conversación» (pide un segundo
  clic).
- **VOZ · KOKORO:** voz, variante (`int8`/`fp32`), velocidad y pausas de
  frase y cláusula. Las rutas de modelo y voces quedan como configuración
  avanzada.
- **TRANSCRIPCIÓN · WHISPER:** motor (`Automático`, `GPU · Vulkan` o `CPU`),
  modelo en una **lista de los modelos instalados**, indicación, tiempo máximo y
  arranque automático. El puerto local se elige libre al arrancar.
- **LLM · API:** proveedor, nombre del perfil, **compatibilidad** (OpenAI o
  Anthropic), dirección, clave, **modelo escrito a mano**, **modelos de la API**
  (la lista real del proveedor), temperatura, máximo de tokens y razonamiento.
- **AGENTE:** abre `system.md` o `voice.json` con el editor predeterminado para
  cambiar el comportamiento o los perfiles de modelo/esfuerzo de cada agente.
- **ATAJOS:** los cinco atajos globales editables en línea (llamada, colgar,
  repetir, silencio y ajustes). Se validan al guardar, no pueden repetirse y se
  re-registran sin reiniciar; si otra app usa el mismo atajo, queda en el log.
- Guardar y «cambios sin guardar» quedan siempre al pie de la ventana.

Los ajustes de audio, voz, TTS, VAD, Whisper y conversación se guardan en
`%APPDATA%\EzAgent\voice.json`; los perfiles de proveedor, en
`%APPDATA%\EzAgent\llm.json`. No hay una segunda ventana de configuración.
La posición de la barra se guarda como `drop_x`/`drop_y` (con `drop_layout`) en
`voice.json` y se restaura al abrir la app.

## Núcleo agéntico

EzAgent no solo delega en Herdr: también puede trabajar con archivos, buscar y
leer la web, cargar skills, recordar preferencias y controlar la escucha. Todo
pasa por una política de permisos y queda en una auditoría.

### Carpetas

```
%APPDATA%\EzAgent\
  voice.json            ajustes del mensajero y posición de la barra
  llm.json              perfiles de proveedor
  permissions.toml      política de rutas, niveles y confirmaciones
  system.md             comportamiento y estilo (editable a mano)
  memory\USER.md        quién eres (1.375 caracteres)
  memory\MEMORY.md      hechos y flujo de trabajo (2.200 caracteres)
  memory\history.db     historial buscable (SQLite FTS5, 180 días)
  skills\               skills propias (una carpeta por skill con SKILL.md)
  tools.d\              herramientas declarativas extra (*.toml)
  logs\audit.jsonl      auditoría append-only, rota a 5 MB
  journal\              operaciones deshacibles
~\.agents\skills\       skills globales compartidas (solo lectura)
~\EzAgentWorkspace\     carpeta de trabajo por defecto
```

`permissions.toml` y `system.md` se crean con valores seguros la primera vez.
Se editan con un editor de texto; el agente nunca escribe `permissions.toml`.
`system.md` se recarga al inicio de cada turno, sin reiniciar.

### Herramientas

- **Hora y archivos:** `get_datetime`, `list_dir`, `read_file`, `create_dir`,
  `write_file`, `copy_path`, `move_path`, `delete_path`, `open_path`,
  `undo_last`. Borrar siempre manda a la **Papelera**; nunca hay borrado
  definitivo.
- **Web:** `web_search` y `fetch_page` con bloqueo de `localhost`, IPs privadas
  y redirecciones peligrosas. El contenido web se entrega al modelo como dato
  no confiable y activa `taint`: en ese turno, cualquier escritura pide
  confirmación.
- **Skills:** `load_skill` y `run_skill_script`. Se descubren en
  `%APPDATA%\EzAgent\skills\` y `~\.agents\skills\`; el catálogo se inyecta en
  el prompt. Los scripts están desactivados por defecto
  (`allow_scripts = false`) y, si se activan, piden confirmación y se aprueban
  por hash de la carpeta.
- **Memoria:** `remember` propone y solo guarda tras un sí; `recall` busca en el
  historial; `forget` y `compact_memory` piden confirmación. Para borrar todo
  el historial:
  `python -m ezagent.agent.memory wipe` (pide escribir «borrar»; `--yes` para
  modo no interactivo).
- **Herdr:** `delegate_to_agent` (listar, leer, enviar, lanzar, enfocar,
  vigilar o matar) y `voice_control` (cerrar escucha, silenciar, repetir).

### Confirmaciones

Las operaciones destructivas (borrar, sobrescribir, matar un agente) siempre
piden confirmación. El toggle **CONVERSACIÓN · Confirmar acciones** solo añade
confirmación a las operaciones de nivel 2 no destructivas. La pregunta se lee
por voz y se muestra en la barra (🎤 pasa a ✓ y ✕ es «no»); también se
puede responder «sí», «confirmo», «adelante» o «dale». Para operaciones
masivas hay que decir «confirmo» y el número de elementos. Si no hay respuesta
en 15 s, se cancela.

### Skills incluidas

`organizar-carpetas`, `investigar-web`, `documentos-word`, `documentos-pdf`,
`hojas-excel`, `resumir-documento` y `notas-rapidas`. `documentos-word`,
`documentos-pdf` y `hojas-excel` necesitan `uv sync --extra office`. En el
ejecutable portable, los
scripts de skills buscan un Python del sistema; sin él, las skills que solo
leen/escriben archivos siguen funcionando y las que ejecutan scripts muestran
un error claro.

## Tareas pendientes y avisos

Cuando pides algo como «vigila esta tarea y avísame cuando termine», EzAgent
guarda la dirección del agente y **la tarea concreta** en
`%APPDATA%\EzAgent\tasks.json`. Esa tarea:

- se inyecta en el prompt como `<pending_tasks>` para que el asistente sepa qué
  está esperando;
- se conserva si reinicias EzAgent;
- se usa en el aviso de finalización: «Oye, ya terminé «…». ¿Quieres que te
  resuma qué hizo, que te lea la salida o que te la muestre?»;
- se elimina cuando el watcher avisa.

Puedes preguntar «¿qué tareas pendientes tengo?» y el asistente las enumera.

### Modelo y esfuerzo por agente

Cada agente delegado tiene su propio perfil en `voice.json`, independiente del
modelo del mensajero de voz. `delegate_to_agent` acepta `model` y `effort` en
`action=run`; los flags se añaden solo al comando de ese agente:

```json
"agents": {
  "claude": ["claude"],
  "opencode": ["opencode"],
  "mcode": ["mcode"],
  "codex": ["codex"],
  "kimi": ["kimi"]
},
"agent_profiles": {
  "claude": {
    "models": ["sonnet", "opus", "haiku"],
    "efforts": ["low", "medium", "high"],
    "default_model": "",
    "default_effort": "",
    "model_args": ["--model", "{model}"],
    "effort_args": ["--effort", "{effort}"]
  },
  "opencode": {
    "models": ["deepseek-v4.1-flash"],
    "efforts": ["low", "medium", "high"],
    "default_model": "",
    "default_effort": "",
    "model_args": ["--model", "{model}"],
    "effort_args": ["--effort", "{effort}"]
  }
}
```

`default_model` y `default_effort` vacíos significan «solo aplica modelo o
esfuerzo cuando el usuario o el LLM los piden». Si quieres un valor fijo para
un agente, rellénalos. Ajusta `model_args`/`effort_args` a lo que acepte cada
CLI (por ejemplo, Codex usa `-c model_reasoning_effort={effort}`). Las claves
de `agents` son libres: puedes añadir `kimi`, `deepseek`, `minimax` o el agente
que uses, y crear su perfil.

### Encadenar la salida de un agente en otro

Pasa `input_from` al segundo `action=run` con la dirección del primero. Si el
primero sigue trabajando, EzAgent guarda la cadena, espera a que termine y lanza
el segundo con su salida como contexto; si ya terminó, la pasa en el mismo
turno. El destino conserva su propio modelo/esfuerzo y, al acabar, da el aviso
con la tarea concreta.

Ejemplo por voz:

> «Abre Claude Code en esfuerzo low con Sonnet y pásale su salida a OpenCode
> con DeepSeek en esfuerzo alto para que haga el plan. Avísame al terminar.»

## Proveedor y modelo LLM

Se cambian desde el grupo **LLM · API** del menú o con variables de entorno y
`llm.json`. Presets: **OpenCode Go, OpenAI (GPT, Codex), Claude (Anthropic),
Gemini, Groq, Cerebras, DeepSeek y OpenRouter**, más **Personalizado** para
cualquier servicio compatible con la API de OpenAI o con la de Anthropic (con
el nombre de perfil que elijas). El modelo se escribe a mano; «Modelos de la
API» trae la lista real del proveedor y, si escribes un id sin prefijo
(`qwen3-32b` en lugar de `qwen/qwen3-32b`), EzAgent lo completa. Ejemplo por
entorno:

```dotenv
LLM_PROVIDER=groq
GROQ_API_KEY=...
# Opcionales; si faltan, se usan los del proveedor:
# LLM_MODEL=openai/gpt-oss-120b
# LLM_BASE_URL=https://api.groq.com/openai/v1
```

Las suscripciones de Claude Code o Codex (inicio de sesión con la cuenta) no
son claves de API: para usar Claude o GPT aquí hace falta una clave de API.
Los planes gratuitos con pocos tokens por minuto (Groq gratis: 7.000–8.000) no
alcanzan, porque cada petición con herramientas ronda los 7.000 tokens.
Guía completa: [`docs/llm-providers.md`](docs/llm-providers.md).

## Atajos

| Atajo | Acción |
|---|---|
| `Ctrl+Alt+Space` | Iniciar / colgar la llamada |
| `Ctrl+Alt+E` | Colgar (`Ctrl+Alt+C` sigue como alias) |
| `Ctrl+Alt+M` | Pausar / reactivar la escucha |
| `Ctrl+Alt+O` | Abrir / ocultar la ventana de ajustes |
| `Ctrl+Alt+R` | Repetir el último resumen |

El micrófono no se cierra entre turnos durante una llamada. Las acciones
explícitas de nivel 0 y 1 se ejecutan directamente y el asistente responde en
una frase corta con qué hace; las destructivas o masivas piden confirmación
aunque el usuario haya dado la orden.

## Pruebas

```powershell
uv run pytest -q                # 840 pruebas
uv run ruff check src tests     # lint
```

Arquitectura del flujo completo: [`docs/architecture.md`](docs/architecture.md).
Detalle del mensajero, latencia y límites:
[`src/ezagent/voice/README.md`](src/ezagent/voice/README.md).

## Comunidad

- Licencia: [Apache-2.0](LICENSE).
- Contrato para agentes de código que modifican el repositorio:
  [AGENTS.md](AGENTS.md).
