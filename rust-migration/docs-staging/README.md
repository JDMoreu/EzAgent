# EzAgent

**Capa de voz local para agentes de código.** Hablas; EzAgent entiende la
intención, delega la tarea en Claude Code, OpenCode, Codex u otro CLI
(directamente o a través de Herdr), vigila el progreso y te avisa por voz cuando
termina o se bloquea. No es un asistente general: el trabajo real lo hacen los
agentes delegados.

Esta es la **versión Rust**: un solo ejecutable, `ezagent-voice.exe`, sin Python
ni Tk en tiempo de ejecución. Lee y escribe los mismos archivos de datos que la
versión anterior (`voice.json`, `llm.json`, `permissions.toml`, tareas,
historial, tablero del enjambre), así que puedes cambiar de una a otra sin
migrar nada. Mientras el repositorio conserve la versión Python de referencia,
se abre con `EzAgent.bat py`.

La interfaz es una **barra flotante** sin texto: una cápsula (o un círculo) con
un gradiente vivo, **Marea**, que fluye con tu voz y con la del asistente, una
ventana de ajustes centrada que se abre desde ☰ y un **chat escrito** que nace
de la propia barra. No hay app de escritorio, dashboard ni panel de llamada.

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
             ├─ modo autónomo (elefante y pez dorado)
             └─ Herdr: OpenCode · Claude Code · Codex · enjambre
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
| STT | whisper.cpp + `large-v3-turbo` | Local, GPU AMD vía Vulkan o CPU |
| LLM | Proveedor configurable, compatible con OpenAI o con Anthropic | Nube |
| TTS | Kokoro v1.0, voces españolas, en cola | Local, CPU (ONNX) |

El pipeline es propio (VAD, whisper.cpp, Kokoro y el LLM); no usa Pipecat. Si
existe la caché antigua `~/.cache/pipecat/kokoro-onnx`, los modelos Kokoro se
copian una sola vez a `%LOCALAPPDATA%\EzAgent\models\kokoro`.

Recorrido completo de un turno, mapa de crates, hilos y reglas de cerrojos:
[`docs/architecture.md`](docs/architecture.md).

## Instalación

Requisitos: Windows 11 de 64 bits y una clave del proveedor LLM que elijas.

### Instalador 0.2.0

El instalador se genera con `packaging\build-rust.ps1` y sale en
`dist\EzAgent-Setup-0.2.0.exe`. Instala en
`%LOCALAPPDATA%\Programs\EzAgent` **sin pedir administrador**, añade accesos en
el menú Inicio y, si lo marcas, en el escritorio, y no instala Python. Usa el
mismo identificador de producto que la versión Python, así que una actualización
reemplaza a la anterior y conserva tus datos.

1. Ejecuta `dist\EzAgent-Setup-0.2.0.exe` y sigue el asistente.
2. Abre **EzAgent** desde el menú Inicio.
3. La primera vez se crean `%APPDATA%\EzAgent`, la política segura
   (`permissions.toml`) y `system.md`. Elige el proveedor y la clave en
   **Ajustes → Modelos**.
4. La barra aparece abajo al centro y empieza escuchando.

También hay una **versión portable**: `dist\EzAgent-portable-0.2.0.zip` se
descomprime en cualquier carpeta y se ejecuta `ezagent-voice.exe`. Las DLL de
ONNX Runtime y espeak-ng van incluidas en las dos variantes.

### Modelos

Los modelos de **Kokoro** y **Smart Turn** se descargan solos a
`%LOCALAPPDATA%\EzAgent` la primera vez (con conexión), con verificación SHA256. **whisper.cpp
y su modelo no vienen dentro del instalador**: se instalan una vez con el
descargador del repositorio (`uv run python -m ezagent.core.downloads --all`) o
se copian a `%LOCALAPPDATA%\EzAgent\whisper`. Si tienes un `whisper-server.exe`
propio con Vulkan, ponlo en esa carpeta (o en `cpu\`) y se usará antes que el de
CPU; otra ubicación se indica con `EZAGENT_WHISPER_HOME` o `whisper_home` en
`voice.json`.

### En desarrollo

```powershell
git clone <url-del-repositorio> EzAgent
cd EzAgent
uv sync                     # DLL de ONNX Runtime y espeak-ng, y referencia Python
cd rust
cargo build --release -p ez-app --bin ezagent-voice
cargo build --release -p ez-swarm --bin ezs
cd ..
EzAgent.bat
```

`EzAgent.bat` sin argumentos abre el menú (opción 1 inicia el mensajero); con
`EzAgent.bat voice` arranca directo. `EzAgent.bat install` hace `uv sync` y
compila la versión Rust (incluido `ezs`); `EzAgent.bat py <comando>` fuerza la
ruta Python.
`rust\run-dev.cmd` lanza el ejecutable con las variables de las DLL y sirve para
probar una compilación de depuración. La primera vez, `EzAgent.bat` crea `.env`
desde `.env.example` y lo abre para que pongas `LLM_PROVIDER` y su clave.

## La barra

La barra es **una cápsula (o un círculo) anclada abajo al centro**, sobre la
barra de tareas. El contorno es fijo y nítido; lo que se mueve es el interior:
**Marea**, un gradiente orgánico como tintas que se mezclan en el agua (verde
azulado, aguamarina y menta, con filamentos melocotón).

| Estado | Cómo se mueve |
|---|---|
| En espera | Fluye y se pliega despacio |
| Te escucha | Tu voz lo acelera y lo empuja hacia arriba; entra una luz desde abajo |
| Habla EzAgent | Lo recorren pulsos de luz al ritmo de su voz |
| Piensa | Gira despacio, como un remolino (tonos índigo y cian) |
| Pregunta · error · pausa | Ámbar · rojo · gris lento |

Los colores siguen al estado y al tema, claro u oscuro; con movimiento reducido
el fluido queda quieto. Se arrastra por la forma y recuerda su posición
(`drop_x`/`drop_y`, `drop_layout` y `drop_dock` en `voice.json`).

Los botones solo aparecen al acercar el puntero: la forma crece un poco y **☰**
(Ajustes), **💬** (chat), **🎤** (micrófono) y **✕** nacen desde detrás de ella,
escalonados. **✕ es dinámico:** con la llamada en marcha cuelga; al colgar, el
micrófono se pliega y ✕ se convierte en el botón verde azulado de **llamar**,
que vuelve a abrir la conversación. Una confirmación oculta ☰ y 💬, convierte 🎤
en ✓ y deja ✕ como «no» (la pregunta se dice por voz). La barra pinta a 60 fps
mientras algo se mueve.

**El micrófono tiene tres estados:**

| Estado | Cómo se ve | Qué escucha | Cómo se sale |
|---|---|---|---|
| Escuchando | 🎤 normal | Todo | — |
| En pausa | 🎤 tachado | Solo la frase de reactivación | Clic en 🎤 o decir «EzAgent, escúchame» |
| Colgado | Sin 🎤; ✕ pasa a «llamar» | Nada | Clic en «llamar» o `Ctrl+Alt+Space` |

«Cierra la escucha», «pausa» o «silencia el micro» dejan la escucha **en pausa**;
«adiós» o «cuelga» cuelgan. En pausa el micrófono sigue abierto en local, pero
solo se transcriben frases cortas (≤ 3 s) y solo cuentan si la frase entera es
la de reactivación: tiene que nombrar a EzAgent y decir el verbo («EzAgent,
escúchame», «EzAgent, vuelve», «EzAgent, abre la escucha»). Lo que no coincide
se descarta sin llegar al modelo ni al log. Si apagas «Reactivar por voz», la
pausa es un silencio total y solo el botón la quita. El modelo ya no puede
silenciar el micrófono: si lo pide, la escucha queda en pausa (siempre
reversible).

**Círculo o píldora:** cámbialo en Ajustes → Aspecto o con «Cambiar forma» en el
menú de la bandeja. La forma, el tema, el movimiento y la escala se aplican al
instante; el resto se guarda con **Guardar**.

### Chat escrito

El botón **💬** o `Ctrl+Alt+T` abre el chat. El panel nace de la forma con una
animación corta y se cierra con su botón o el mismo atajo. Escribir es un turno
más: el mensaje va al mismo modelo y a las mismas herramientas que la voz, y la
respuesta se puede leer, copiar y escuchar. El historial vive en
`%APPDATA%\EzAgent\history\chat.jsonl`; desde la cabecera puedes abrir otra
conversación, crear una nueva o borrarla (pide confirmación). El chat dibuja
Markdown (títulos, listas, tablas, código) y muestra imágenes locales que el
asistente mencione; al pulsarlas se abren con el visor del sistema.

### Interrumpir al asistente

Mientras EzAgent habla, la interrupción es **adaptativa**: la app publica la
envolvente de lo que está sonando y compara cada bloque del micrófono con el eco
esperado, así que tu voz normal corta la respuesta sin que un ventilador o un
golpe de teclado la corten. El umbral aprende el acople de tus altavoces y tu
micrófono al principio de cada frase. En Ajustes → Voz puedes apagar la
interrupción; el control «Umbral de interrupción» de las opciones avanzadas se
conserva por compatibilidad y, en 0, también la desactiva. La primera frase de
una sesión es algo más conservadora (0,6 s) mientras mide el acople.

## Ajustes

La ventana de Ajustes se abre **en el centro la primera vez** y luego donde la
dejaste, con ☰ o `Ctrl+Alt+O`. Se puede arrastrar desde cualquier zona libre,
redimensionar desde los bordes y las esquinas, y un doble clic en la cabecera
devuelve el tamaño y la posición de fábrica; la geometría se recuerda en
`voice.json`. A la izquierda
están las cinco secciones y a la derecha los controles de la elegida, con todo el
alto de la ventana, así que casi nunca hace falta desplazarse. Si una sección no
cabe, la rueda desplaza su contenido. Los campos de texto se editan en línea: al
pulsarlos, la ventana pide el teclado y lo devuelve al confirmar con `Enter`,
cancelar con `Esc` o pulsar fuera. `Ctrl+V` pega desde el portapapeles.

- **ASPECTO:** forma (píldora o círculo), escala de la interfaz, movimiento
  (según el sistema, completo o reducido) y tema (automático, claro, oscuro o
  **personalizado**). El tema personalizado parte de una paleta (Marea,
  Atardecer, Bosque, Lavanda o Medianoche) y permite editar los cuatro colores
  del gradiente (`#RRGGBB`): principal, secundario, acento y fondo.
- **ESCUCHA:** micrófono, **medidor de entrada con nivel en vivo y umbral
  arrastrable**, umbral de ruido, fin de turno (espera de silencio y cierre
  inteligente) y reactivación por voz con su frase y qué hace «cierra» (pausa o
  cuelga).
- **VOZ:** voz de Kokoro y velocidad; conversación (interrupción, confirmar
  acciones y voz del chat) y acciones de prueba: «Prueba de voz», «Repetir lo
  último» y «Nueva conversación».
- **MODELOS:** motor y modelo de Whisper, arranque automático, indicación y
  diccionario; proveedor y perfil del LLM, **modelos de la API**, modelo escrito
  a mano, clave, compatibilidad, dirección y nombre del perfil.
- **SISTEMA:** los seis atajos globales, **Opciones avanzadas** (afinado de
  escucha, voz, transcripción y conexión del LLM) y **Archivos**: abrir la
  carpeta de datos, `system.md`, `permissions.toml` o `voice.json` con el editor
  predeterminado.

Guardar y el punto de «cambios sin guardar» están siempre al pie de la ventana.
La apariencia y algunas opciones de escucha (reactivación y qué hace «cierra») se
aplican en vivo, pero el punto recuerda que falta guardar; el resto se aplica al
pulsar Guardar. Los ajustes de audio, voz, TTS, VAD, Whisper y conversación se
guardan en `%APPDATA%\EzAgent\voice.json`; los perfiles de proveedor, en
`%APPDATA%\EzAgent\llm.json`.

## Bandeja del sistema

Mientras EzAgent está abierto aparece un icono en la bandeja de Windows con el
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

## Núcleo agéntico

EzAgent no solo delega en Herdr: también puede trabajar con archivos, buscar y
leer la web, cargar skills, recordar preferencias, controlar la escucha y llevar
un modo autónomo. Todo pasa por una política de permisos y queda en una
auditoría.

### Carpetas

```
%APPDATA%\EzAgent\
  voice.json            ajustes del mensajero y posición de la barra
  llm.json              perfiles de proveedor (con claves; ignorado por git)
  permissions.toml      política de rutas, niveles y confirmaciones
  system.md             comportamiento y estilo (editable a mano)
  tasks.json            tareas vigiladas
  history\chat.jsonl    historial del chat escrito
  memory\USER.md        quién eres (1.375 caracteres)
  memory\MEMORY.md      hechos y flujo de trabajo (2.200 caracteres)
  memory\history.db     historial buscable (SQLite FTS5, 180 días)
  skills\               skills propias (una carpeta por skill con SKILL.md)
  tools.d\              herramientas declarativas extra (*.toml)
  logs\engine.log       registro de la app (rota a diario)
  logs\audit.jsonl      auditoría append-only, rota a 5 MB
  logs\latency.jsonl    tiempos de cada turno
  journal\              operaciones deshacibles
  swarm\                board.db, endpoint.json, asientos y traspasos del enjambre
  autonomo\             encargos, criterios, veredictos y resúmenes del modo autónomo
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
  y redirecciones peligrosas. El contenido web se entrega al modelo como dato no
  confiable y activa `taint`: en ese turno, cualquier escritura pide
  confirmación.
- **Skills:** `load_skill` y `run_skill_script`. Se descubren en
  `%APPDATA%\EzAgent\skills\` y `~\.agents\skills\`; el catálogo se inyecta en
  el prompt. Los scripts están desactivados por defecto
  (`allow_scripts = false`) y, si se activan, piden confirmación y se aprueban
  por hash de la carpeta.
- **Memoria:** `remember` propone y solo guarda tras un sí; `recall` busca en el
  historial; `forget` y `compact_memory` piden confirmación.
- **Herdr:** `delegate_to_agent` (listar, leer, enviar, lanzar, enfocar,
  vigilar o matar), `voice_control` (cerrar escucha, silenciar, repetir) y
  `swarm` (montar y dirigir un enjambre).
- **Modo autónomo:** `start_autonomous_task`, `start_autonomous_team`,
  `autonomous_status`, `resume_autonomous_task` y `stop_autonomous_task`.

### Confirmaciones

Las operaciones destructivas (borrar, sobrescribir, matar un agente) siempre
piden confirmación. El interruptor **CONVERSACIÓN · Confirmar acciones** solo
añade confirmación a las operaciones de nivel 2 no destructivas. La pregunta se
lee por voz y se muestra en la barra (🎤 pasa a ✓ y ✕ es «no»); también se puede
responder «sí», «confirmo», «adelante» o «dale». Para operaciones masivas hay
que decir «confirmo» y el número de elementos. Si no hay respuesta en 15 s, se
cancela.

### Skills incluidas

`organizar-carpetas`, `investigar-web`, `documentos-word`, `documentos-pdf`,
`hojas-excel`, `resumir-documento` y `notas-rapidas`. En la versión instalada,
los scripts de skills buscan un Python del sistema; sin él, las skills que solo
leen/escriben archivos siguen funcionando y las que ejecutan scripts muestran un
error claro.

## Agentes delegados

`delegate_to_agent` es la vía normal del modelo hacia los CLIs de agentes; el
modo autónomo y el enjambre usan el mismo adaptador de Herdr:

- **run:** arma el comando desde el perfil de `voice.json` (`agents`,
  `agent_profiles`), con modelo y esfuerzo validados. Abre una pestaña visible de
  Herdr, espera a que el programa arranque, fija modelo y esfuerzo con sus
  comandos internos y pega la tarea.
- **send:** envía la tarea a un agente ya abierto tal cual, sin preámbulo.
- **read / get / list / focus / watch / kill:** leer la pantalla, consultar,
  enfocar, vigilar o cerrar.
- **Cadenas** (`input_from`, `then`): cuando un agente termina, su salida pasa
  al siguiente como dato.

Cada agente tiene su propio perfil, independiente del modelo del mensajero. En
`voice.json`:

```json
"agents": {
  "claude": ["claude"],
  "opencode": ["opencode"],
  "mcode": ["mcode"],
  "codex": ["codex"],
  "qwen": ["qwen"]
},
"agent_profiles": {
  "claude": {
    "models": ["opus", "sonnet", "haiku"],
    "efforts": ["low", "medium", "high"],
    "default_model": "",
    "default_effort": "",
    "model_args": ["--model", "{model}"],
    "effort_args": ["--effort", "{effort}"]
  }
}
```

`default_model` y `default_effort` vacíos significan «solo aplica modelo o
esfuerzo cuando el usuario o el LLM los piden». Las claves de `agents` son
libres: puedes añadir `kimi`, `deepseek`, `minimax` o el agente que uses, y crear
su perfil. Ajusta `model_args`/`effort_args` a lo que acepte cada CLI.

**Avisos.** Cuando un agente vigilado termina o se bloquea, EzAgent avisa por
voz con la tarea concreta, guardada en `tasks.json`. La salida del agente se
trata siempre como dato no confiable. Al dejar el puntero sobre la barra, la
vista rápida muestra el estado de las terminales vigiladas y los desenlaces
recientes. Hablar o escribir los da por vistos.

**Tareas pendientes.** Cuando pides «vigila esta tarea y avísame cuando
termine», la tarea se guarda en `%APPDATA%\EzAgent\tasks.json`, se inyecta en el
prompt como `<pending_tasks>`, sobrevive a un reinicio y se elimina cuando el
vigilante avisa. Puedes preguntar «¿qué tareas pendientes tengo?» y el asistente
las enumera.

### Nombres mal dictados: «¿Te refieres a…?»

Antes de usar un espacio, una terminal o un agente, EzAgent compara lo que
dictaste con lo que hay abierto en Herdr. Una coincidencia exacta se usa tal
cual; una muy probable se usa y queda constancia en el resultado; una dudosa se
pregunta por voz («¿Te refieres al espacio «TEST»?») y no se toca nada hasta que
respondas. Si no hay parecido suficiente, la herramienta devuelve un error que
lista los nombres vivos. Así «TES» o «OpenCobe» no abren ni leen la terminal
equivocada.

## Modo enjambre

Para trabajo en equipo (uno planifica, otro desarrolla, otro valida y devuelve
correcciones) EzAgent monta un **tablero compartido** al que los agentes hablan
con el comando `ezs`. Se pide por voz, por ejemplo:

> «Monta un enjambre en `C:\proyectos\mi-app`: OpenCode de planificador, Claude
> de desarrollador y Codex de QA. Que implementen el plan y me avisen al
> terminar.»

- Cada miembro abre una etapa que espera a la anterior, salvo los marcados como
  paralelos (backend y frontend a la vez). Se abre **cuando le toca**, no antes.
- Al abrir un agente solo se escribe una línea («Eres «dev»… Ejecuta ahora: ezs
  next»). Su papel y las reglas llegan con el primer `ezs next`, y `ezs me` las
  repite.
- Los mensajes son de 280 caracteres; el detalle va en un archivo (`--ref`).
- El ciclo validar → corregir tiene un tope de 3 rondas (`ezs fix`).
- El supervisor vigila la salud del equipo (agente sin señales, tarea que nadie
  toma, pregunta sin respuesta, tarea fallida) y dice cada aviso una vez.
- Al terminar, el resumen de cada etapa y la pregunta de qué hacer con el
  resultado reabren la escucha.

`ezs.exe` se compila con `cargo build --release -p ez-swarm --bin ezs` y se
instala junto a `ezagent-voice.exe`; el enjambre lo pone en el `PATH` de las
terminales que abre. El tablero vive en `%APPDATA%\EzAgent\swarm\board.db` y solo
escucha en `127.0.0.1` con un token por sesión.

## Modo autónomo

El modo autónomo (modelo «elefante y pez dorado») hace una tarea de principio a
fin sin pedirte permiso en cada paso, pero **solo si lo autorizas tú**. EzAgent
hace de elefante: guarda el encargo y los criterios, lanza al ejecutor, aprueba
los permisos que pide y verifica el resultado con una sesión nueva de
verificación. No ejecuta la tarea por su cuenta.

Se activa con una frase explícita que incluya «modo autónomo», «autorizo»,
«yolo», «sin pedirme permiso» o «sin preguntarme». Por ejemplo:

> «Hazlo en modo autónomo y autorizo los permisos. En la carpeta
> `C:\pruebas\autonomo`, crea `saludo.txt` con «hola» y `nota.txt` con «listo».
> Criterio: cada archivo contiene su texto; se comprueba con `type saludo.txt` y
> `type nota.txt`. Usa Codex en el espacio «pruebas».»

1. EzAgent pide **una sola confirmación generada por el código**, con la tarea,
   los criterios y sus comprobaciones, la carpeta, el agente y los topes.
2. Se responde «sí» o «no», por voz o con los botones de la barra. La
   autorización solo sirve para las terminales de esa tarea y se aprueba **una
   sola vez** en cada diálogo (nunca «permitir siempre»).
3. El ejecutor trabaja; al terminar, un **verificador nuevo** comprueba cada
   criterio con evidencia. Hacen falta **dos verificaciones limpias seguidas**.
4. Si no cumple, EzAgent escribe `correccion-N.md` con los hallazgos aceptados y
   se lo manda al ejecutor. Los hallazgos son datos, no instrucciones.
5. Al terminar avisa y deja `privado\resumen.md` con la evidencia y el diff. La
   carpeta de la tarea está en `%APPDATA%\EzAgent\autonomo\<id>\`; lo que se
   comparte con los agentes está en `compartido\`, y lo privado (estado,
   bitácora y resumen) en `privado\`.

**Pararlo:** di «para el modo autónomo», pulsa `Ctrl+Alt+E` o haz clic en la
fila «Autónomo · ronda N» de la vista rápida y confirma. La autorización se
retira al instante y se cierran las terminales de los verificadores; la del
ejecutor se deja abierta porque ahí está su trabajo. **Pausarlo:** los topes por
defecto son 5 rondas, 120 minutos y 200 aprobaciones; al llegar, ante un
problema repetido, un diálogo desconocido o un incidente de cuota, se pausa y
pregunta. Di «sigue» para reanudar o «para» para detenerlo.

Una sola tarea autónoma a la vez. La lista de acciones prohibidas (borrar la
raíz o el perfil, formatear, `git push --force` a una rama principal, `| sh`,
apagar el equipo, tocar credenciales…) vive en el código: si aparece, no se
aprueba y se pregunta. Cada aprobación automática queda en la auditoría y en la
bitácora.

## Proveedor y modelo LLM

Se cambian desde **Ajustes → Modelos** o con variables de entorno y `llm.json`.
Presets: **OpenCode Go, OpenAI (GPT, Codex), Claude (Anthropic), Gemini, Groq,
Cerebras, DeepSeek y OpenRouter**, más **Personalizado** para cualquier servicio
compatible con la API de OpenAI o con la de Anthropic. El modelo se escribe a
mano; «Modelos de la API» trae la lista real del proveedor y, si escribes un id
sin prefijo (`qwen3-32b` en lugar de `qwen/qwen3-32b`), EzAgent lo completa
cuando la coincidencia es única. Ejemplo por entorno:

```dotenv
LLM_PROVIDER=groq
GROQ_API_KEY=...
# Opcionales; si faltan, se usan los del proveedor:
# LLM_MODEL=openai/gpt-oss-120b
# LLM_BASE_URL=https://api.groq.com/openai/v1
```

Las suscripciones de Claude Code o Codex (inicio de sesión con la cuenta) no son
claves de API: para usar Claude o GPT aquí hace falta una clave de API. Los
planes gratuitos con pocos tokens por minuto (Groq gratis: 7.000–8.000) no
alcanzan, porque cada petición con herramientas ronda los 7.000 tokens. Guía
completa: [`docs/llm-providers.md`](docs/llm-providers.md).

## Atajos

| Atajo | Acción |
|---|---|
| `Ctrl+Alt+Space` | Iniciar / colgar la llamada |
| `Ctrl+Alt+E` | Colgar (`Ctrl+Alt+C` sigue como alias) |
| `Ctrl+Alt+M` | Pausar / reactivar la escucha |
| `Ctrl+Alt+O` | Abrir / ocultar Ajustes |
| `Ctrl+Alt+R` | Repetir el último resumen (`Ctrl+Alt+Y` como alias) |
| `Ctrl+Alt+T` | Abrir / cerrar el chat |

Los seis se editan en Ajustes → Sistema. Se validan al guardar, no pueden
repetirse y se re-registran sin reiniciar; si otra app usa el mismo atajo, queda
en el log. El micrófono no se cierra entre turnos durante una llamada.

## Pruebas

Desde `rust/`:

```powershell
cargo test --workspace
cargo test -p ez-app --features replay --test replay_parity
```

La segunda corrida compara los 25 escenarios de punta a punta con las trazas
doradas de Python (unos 7 minutos). El arnés también está como binario:

```powershell
cargo build -p ez-fakes --bins
cargo build -p ez-swarm --bin ezs
cargo run -p ez-app --features replay --bin ezagent-replay -- --all
cargo run -p ez-app --features replay --bin ezagent-replay -- --scenario 05 --trace-out salida.jsonl
```

`--list` enumera los escenarios y `--verify` corre cada uno dos veces para
comprobar que la traza es idéntica. Para comprobar que los fixtures no han
cambiado:

```powershell
powershell -File parity/regen.ps1 -Check
```

El Python de `py-baseline` sigue siendo el oráculo y se mantiene en verde:

```powershell
uv run pytest -q
uv run ruff check src tests
```

La puerta de una tarea de la migración es
`powershell -File rust/gate.ps1 -Task <ID>` (alcance del diff, fmt, clippy,
pruebas del crate, pruebas del workspace y pytest).

Arquitectura del flujo completo: [`docs/architecture.md`](docs/architecture.md).

## Comunidad

- Licencia: [Apache-2.0](LICENSE).
- Contrato para agentes de código que modifican el repositorio:
  [AGENTS.md](AGENTS.md).
