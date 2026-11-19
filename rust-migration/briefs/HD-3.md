# HD-3 — ez-herdr: validar tipo, modelo y esfuerzo contra el catálogo

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` (§2, §5).**
En paralelo, HD-2 toca `ez-herdr/src/herdr/actions.rs`, `herdr.rs` y crea `herdr/child.rs`:
no los toques.

## Archivos que se tocan (solo estos)
- Nuevo `rust/crates/ez-herdr/src/driver/choice.rs` (+ `driver/choice/tests.rs`).
- `rust/crates/ez-herdr/src/driver.rs`: **solo** `mod choice;`, el `pub use` de `Choice` y,
  si lo necesitas, hacer `pub(crate)` alguna función privada que ya existe.
- `rust/crates/ez-herdr/src/herdr/process.rs`: **solo** si `which` no acepta una ruta
  absoluta a un ejecutable que existe (debe aceptarla: las pruebas de punta a punta ponen
  la ruta completa del agente falso como comando del catálogo). Prueba incluida.

## Contrato (nombres exactos)
```rust
/// Tipo, modelo y esfuerzo ya validados para delegar en un subagente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// Clave del catálogo (`opencode`, `qwen`…).
    pub key: String,
    /// Etiqueta para personas (`OpenCode`, `Qwen Code`…).
    pub label: String,
    /// Modelo pedido (recortado), o None para el de por omisión.
    pub model: Option<String>,
    /// Esfuerzo pedido (recortado), o None.
    pub effort: Option<String>,
    /// Avisos (modelo sin verificar…).
    pub warnings: Vec<String>,
}
impl<H: SessionHost> AgentDriver<H> {
    /// Valida sin escribir nada (§5). Err = texto para la persona.
    pub fn validate_choice(&self, agent: &str, model: Option<&str>, effort: Option<&str>)
        -> Result<Choice, String>;
}
```

## Reglas (§5 del diseño; mensajes exactos)
1. Tipo con los alias de `resolve_agent`. Desconocido → «No conozco el agente «X».
   Agentes del catálogo: a, b, c.» (claves de `agents` ordenadas, separadas por «, »).
   Programa no encontrado (`which` del primer elemento del comando del tipo) → ««X» está
   en el catálogo pero no encuentro su programa (prog).» (X = clave).
2. Modelo, si se pidió y no está vacío:
   - perfil con `models` no vacío → debe coincidir sin distinguir mayúsculas con uno de
     ellos, aceptando también `<model_prefix><m>` y `proveedor/<m>`. Si no → «<label> no
     tiene el modelo «M». Modelos: a, b, c.»
   - perfil con `discover = "mcode"`: usa `discover_mcode_models` + `match_mcode_model`
     (ya existen); lista vacía → aviso «No pude leer los modelos de <label>; uso «M» sin
     verificar.»; sin coincidencia → «<label> no tiene el modelo «M». Modelos: …».
   - perfil con `discover = "codex"`: usa `discover_codex_models` y la lógica de
     `resolve_codex` (incluye los niveles por modelo); catálogo vacío → el aviso «No pude
     leer los modelos de <label>; uso «M» sin verificar.».
   - sin lista ni descubrimiento → aviso «El catálogo de <label> no lista modelos; uso «M»
     sin verificar.»
   - formato inseguro (`check_value`) → su mensaje.
3. Esfuerzo, si se pidió: `check_value` (lista `efforts`), `model_efforts` del modelo y
   niveles de Codex, con los mensajes que ya existen. Perfil sin `efforts`, sin
   `model_efforts`, sin `effort_args`, sin `env` y sin `prelaunch` de esfuerzo → «<label>
   no tiene niveles de esfuerzo.»
4. **Nunca** llama a `set_mcode_effort`, `prepare_opencode_state` ni escribe archivos.

## Pruebas (con `AgentDriver::with_hooks` y ganchos falsos)
Los seis tipos del catálogo de fábrica, un caso válido y uno inválido de modelo y de
esfuerzo cada uno donde aplique: Claude Code (`sonnet`/`low` válido; `gpt-5` y `ultra`
inválidos), OpenCode (`deepseek-v4.1-flash`/`high` válido, también con `opencode-go/`;
`deepseek-v4-pro`/`low` inválido por modelo), Qwen Code (`qwen3-coder-plus` válido; con
esfuerzo → «no tiene niveles de esfuerzo»), Codex (modelo y nivel del catálogo falso
válidos; modelo o nivel ausentes inválidos; catálogo vacío → aviso), mcode (modelo
descubierto válido; ausente inválido; sin descubrimiento → aviso), Pi (modelo libre →
aviso; esfuerzo `medium` válido, `ultra` inválido). Tipo desconocido y alias (`open code`,
`cloud code`). Programa no instalado. Comprueba que ningún gancho de escritura se llamó.

## Criterios de aceptación
fmt, clippy y test de `ez-herdr` en verde. Informe en `rust-migration/reports/HD-3-r1.md`.
