# IR-2b — Corrección de la revisión de IR-2: OpenCode lee datos viejos y se da por agotado

Revisión del agente principal sobre `work/IR-2` (c340da4, a883ec4, cf5b426). Bien: lectores
tolerantes, `planes.toml` que no se sobrescribe, Codex con dato viejo = `desconocido`,
costo de Claude en `null` en vez de inventarlo, fixtures inventados. **Mal**, con OpenCode,
que es justo el agente que más usa la persona hoy:

1. **Lee datos viejos.** Hoy OpenCode corrió unos diez subagentes durante horas con
   `opencode-go/deepseek-v4.1-flash`, y `ez usage` dice 0 tokens y «la base no se actualiza
   desde hace 623 h». Pero `~/.local/share/opencode/opencode.db` (1,5 GB) y su `-wal`
   (86 MB) se escribieron hoy a las 17:08 (comprobado por el elefante con `ls`). 623 h
   atrás es el 24-oct: lo más probable es que la tabla `session`/`message` que consultas ya
   no sea donde la versión actual (opencode v2.3.0) guarda los mensajes, o que la conexión de
   solo lectura no vea el WAL.
2. **Se da por agotado entero por un aviso del plan gratuito.** «Free usage limit reached»
   (de esta mañana, modelo `opencode/hy4-flash-free`) solo agota los modelos **gratuitos**;
   el plan OpenCode Go (DeepSeek) siguió funcionando todo el día. Con `agotado`, la
   recomendación (IR-3) evitaría OpenCode sin motivo.

## Cambios (mismo alcance que IR-2)
1. Encuentra dónde guarda hoy OpenCode los mensajes y tokens: mira **solo la estructura**
   (`sqlite_master`, `PRAGMA table_info`, `MAX(time_…)` por tabla; nunca el contenido de
   los mensajes) y lee de ahí, en solo lectura y viendo el WAL (`mode=ro` por URI;
   **nunca** `immutable=1`). Si hay más de una tabla o esquema, lee la más reciente y
   anota la decisión. El aviso de «no se actualiza» debe basarse en el dato más nuevo real.
2. Uso **por modelo** en OpenCode (`models: [{model, tokens, messages, cost_usd, state}]`);
   si los datos de Claude Code, mcode o Pi traen el modelo, lo mismo.
3. Un aviso de límite agota **el plan o los modelos que nombra** (Free → modelos
   `*-free`; «OpenCode Go usage limit» → modelos `opencode-go/*`), no al agente entero. El
   agente queda `agotado` solo si todos sus modelos usables lo están. Un aviso deja de
   contar al pasar su reinicio; si no trae hora, al pasar su ventana (Free: 24 h; resto:
   la de `planes.toml` o 5 h) o si hay uso posterior **de ese mismo plan**.
4. Pruebas con fixtures inventados: base con el esquema actual (créala en la prueba con
   las mismas tablas y columnas que encuentres) y uso de hoy → tokens > 0 y por modelo;
   aviso Free + uso posterior de `opencode-go/…` → OpenCode no está agotado, el modelo
   gratuito sí; aviso Go con «resets in 2h» ya vencido → no cuenta.

## Entrega
fmt, clippy y test de `ez-scan` y `ez-ctl` en verde. **Ejecuta `ez usage --json` real** y
pega la salida: OpenCode debe mostrar tokens de hoy de `opencode-go/deepseek-v4.1-flash`
distintos de cero y no estar `agotado`. Nunca leas `auth.json` ni el texto de los mensajes.
Commit en `work/IR-2`, informe `rust-migration/reports/IR-2b-r1.md`. Compila en primer
plano y espera el resultado.
