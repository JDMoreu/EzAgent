# Recursos del equipo, inventario de agentes y recomendación (2026-11-19)

Contrato del elefante para la ola IR. Se apoya en `docs/designs/subagentes-herdr.md`
(`ez delegate`, registro y avisos de §13). Rama base: `eco/main`.

## 1. Pedido
- EzAgent mira los **recursos del PC** donde corre (lo vivido hoy: una medición de un
  subagente llegó a 11 GB y dejó al equipo sin memoria).
- El agente padre **reconoce los agentes instalados** y, si la persona no lo define,
  **recomienda** tipo, modelo y esfuerzo para cada delegación, **cuidando los límites de
  uso** de cada agente para no agotarlos.
- Un **`init`** la primera vez escanea agentes, modelos, recursos y sistema.

## 2. Principios
- **Nada gasta cuota**: el escaneo solo lee archivos locales y ejecuta `--version` o
  listados locales (`opencode models`); nunca abre una sesión con modelo ni manda
  mensajes. Las claves y tokens nunca se leen al registro ni se imprimen.
- **La persona manda**: lo que declare (planes, límites, preferencias, tope de subagentes)
  gana sobre lo detectado y sobre la recomendación.
- **Explicable**: cada recomendación dice por qué (papel, nivel del modelo, uso observado,
  recursos).
- **Ejecutable nativo**: se resuelve la ruta real del programa (lección de HD-0c: `env.exe
  opencode` corría el guion de bash de npm).

## 3. `ez init` y el archivo de entorno
`ez init [--refresh] [--json]` escribe `%APPDATA%\EzAgent\entorno.json` (escritura
atómica). `ez delegate` lo ejecuta solo si el archivo falta o tiene más de 7 días.
Contenido:
- **Sistema**: Windows y versión, CPU (modelo, núcleos físicos y lógicos), RAM total,
  disco libre de la unidad de trabajo, GPU (nombre y VRAM dedicada por DXGI), versiones de
  `hc` y `herdr-app`.
- **Agentes**: por cada tipo del catálogo (`claude`, `opencode`, `mcode`/`minimax`,
  `codex`, `qwen`, `pi`): instalado o no, ruta nativa, versión (`--version` con tope de
  10 s), modelos conocidos (catálogo + descubrimiento local: `opencode models`, caché de
  Codex, `config.yaml` de mcode) y niveles de esfuerzo.
- **Uso y límites** (§5) en el momento del escaneo.
Al terminar imprime un resumen para la persona y lo que falta que declare (planes y
límites que no se pudieron detectar).

## 4. Recursos en vivo
- `ez resources [--json]`: RAM libre, memoria virtual libre, CPU %, disco libre y los
  subagentes vivos del padre con su memoria (suma de su árbol de procesos).
- **Guarda en `ez delegate`**: con menos de `min_free_ram_mb` (2 500) o de
  `min_free_commit_mb` (4 000) libres, o con `max_heavy_subagents` (3) subagentes vivos
  compilando o midiendo, **no lanza**: código 4 («Recursos justos: … Espera a que termine
  alguno o usa --force.»). `--force` lo salta con aviso.
- **El vigilante** (`ez watch`) mira la memoria cada vuelta y avisa al padre una vez por
  episodio: `[EzAgent · aviso] Memoria baja: RAM libre N MB, virtual libre M MB. El
  subagente que más usa es <nombre> (X MB).` No mata nada por su cuenta.
- Umbrales en `voice.json` (`resources.*`), editables por la persona.

## 5. Uso y límites de cada agente
- **Declarado por la persona** (`%APPDATA%\EzAgent\planes.toml`): por tipo, nombre del plan,
  ventana (p. ej. 5 h, semanal), prioridad (usar primero / reservar) y, si lo sabe, el
  límite. Se propone al terminar `ez init` con lo detectado; la persona lo confirma.
- **Observado en archivos locales** (fuentes a confirmar en IR-0): Codex guarda
  `rate_limits` (porcentaje usado y reinicio) en sus sesiones; Claude Code guarda el uso de
  tokens por mensaje en `~/.claude/projects`; OpenCode tiene `opencode stats` y registra el
  aviso de límite de 5 h; mcode, Qwen Code y Pi: por investigar.
- `ez usage [--json]`: por agente, uso en la ventana actual (o «desconocido»), límite
  (declarado u observado), cuándo se reinicia y estado: `libre`, `cerca` (≥ 80 %),
  `agotado`, `desconocido`.

## 6. Recomendación
`ez recommend <papel> [--size s|m|l] [--json]` propone tipo + modelo + esfuerzo con sus
razones; `ez delegate <papel>` **sin** `--agent` la usa (y lo dice en la salida). Reglas
(datos en `%APPDATA%\EzAgent\modelos.toml`, con valores de fábrica editables):
- Cada modelo tiene un **nivel** (`alto`, `medio`, `bajo`) y un **costo** (`gratis`,
  `plan`, `pago`).
- Papel → nivel mínimo y esfuerzo: `planner`, `plan_reviewer`, `qa_validator` → alto, high;
  `developer` → medio, medium (`--size l` sube un nivel); `qa` → medio, medium; otro →
  medio.
- Se descartan agentes no instalados y los `agotado`; se penalizan los `cerca` y los
  `desconocido` frente a uno `libre`; a igual nivel gana el más barato y luego la
  prioridad declarada.
- `qa_validator` usa una **familia distinta** a la del último `developer` del mismo padre
  (contra la complacencia que la persona señaló).
- Con recursos justos (§4) se prefiere el agente que menos memoria usa (dato del
  inventario) y se avisa.

## 7. Encargos
- **IR-0** (investigación, sin código del producto): fuentes locales de uso y límites de
  cada agente, cómo obtener versión y modelos sin gastar, formatos y ejemplos sin datos
  sensibles.
- **IR-1**: sistema, recursos en vivo, inventario de agentes, `ez init`, `ez resources`
  y la guarda de `ez delegate`.
- **IR-2**: uso y límites (`planes.toml`, lectores de IR-0, `ez usage`).
- **IR-3**: recomendación (`modelos.toml`, `ez recommend`, `ez delegate` sin `--agent`) y
  avisos de memoria del vigilante.
- **IR-4**: prueba de punta a punta con carpetas de usuario falsas (registros de ejemplo
  de cada agente) y Herdr real.
