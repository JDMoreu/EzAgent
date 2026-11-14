use super::*;

struct TempDir(std::path::PathBuf);

fn cleanup(path: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() {
            cleanup(&child);
        } else {
            let _ = std::fs::remove_file(&child);
        }
    }
    let _ = std::fs::remove_dir(path);
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("ez-herdr-prelaunch-{}-{name}", std::process::id()));
        cleanup(&path);
        std::fs::create_dir_all(&path).expect("directorio temporal");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        cleanup(&self.0);
    }
}

#[test]
fn estado_de_opencode_escribe_model_json() {
    let dir = TempDir::new("opencode");
    let env = prepare_opencode_state_at(&dir.0, "minimax/MiniMax-M3", "high");
    assert_eq!(env["XDG_STATE_HOME"], dir.0.to_string_lossy().as_ref());
    let text = std::fs::read_to_string(dir.0.join("opencode").join("model.json")).expect("json");
    let data: Value = serde_json::from_str(&text).expect("json válido");
    assert_eq!(data["recent"][0]["providerID"], "minimax");
    assert_eq!(data["recent"][0]["modelID"], "MiniMax-M3");
    assert_eq!(data["favorite"], serde_json::json!([]));
    assert_eq!(data["variant"]["minimax/MiniMax-M3"], "high");
    assert!(!text.contains("\\u00e9"));
}

#[test]
fn sin_esfuerzo_no_hay_variante() {
    let dir = TempDir::new("opencode-sin-esfuerzo");
    let env = prepare_opencode_state_at(&dir.0, "proveedor/modelo", "");
    assert!(env.contains_key("XDG_STATE_HOME"));
    let text = std::fs::read_to_string(dir.0.join("opencode").join("model.json")).expect("json");
    let data: Value = serde_json::from_str(&text).expect("json válido");
    assert_eq!(data["variant"], serde_json::json!({}));
}

#[test]
fn referencia_de_modelo_incompleta_se_ignora() {
    let dir = TempDir::new("opencode-incompleto");
    assert!(prepare_opencode_state_at(&dir.0, "sin-barra", "high").is_empty());
    assert!(prepare_opencode_state_at(&dir.0, "/modelo", "high").is_empty());
    assert!(prepare_opencode_state_at(&dir.0, "proveedor/", "high").is_empty());
    assert!(!dir.0.join("opencode").exists());
}

#[test]
fn si_no_se_puede_escribir_devuelve_vacio() {
    let dir = TempDir::new("opencode-bloqueado");
    let blocked = dir.0.join("opencode");
    std::fs::write(&blocked, b"archivo, no carpeta").expect("archivo");
    assert!(prepare_opencode_state_at(&dir.0, "proveedor/modelo", "high").is_empty());
}

#[test]
fn modelos_de_mcode_salen_del_yaml_de_bloque() {
    let yaml = "\
# comentario
provider:
  minimax:
    api_key: secreta
    models:
      MiniMax-M3.1-Flash:
        context: 200k
      \"otro-modelo\":
        context: 1m
  local:
    models:
      llama:
        size: 7b
otra:
  minimax:
    models:
      no-cuenta:
";
    let models = parse_provider_models(yaml);
    assert_eq!(
        models,
        vec![
            (
                "minimax".to_string(),
                vec!["MiniMax-M3.1-Flash".to_string(), "otro-modelo".to_string()]
            ),
            ("local".to_string(), vec!["llama".to_string()]),
        ]
    );
    assert_eq!(
        discover_mcode_models_at_text(yaml),
        vec![
            "minimax/MiniMax-M3.1-Flash",
            "minimax/otro-modelo",
            "local/llama"
        ]
    );
}

fn discover_mcode_models_at_text(text: &str) -> Vec<String> {
    parse_provider_models(text)
        .into_iter()
        .flat_map(|(provider, models)| {
            models
                .into_iter()
                .map(move |model| format!("{provider}/{model}"))
        })
        .collect()
}

#[test]
fn yaml_en_flujo_no_lista_modelos() {
    let yaml = "provider: {minimax: {models: {MiniMax-M3: {}}}}\n";
    assert!(parse_provider_models(yaml).is_empty());
}

#[test]
fn archivo_de_mcode_inexistente_no_lista_modelos() {
    let dir = TempDir::new("mcode-sin-archivo");
    assert!(discover_mcode_models_at(&dir.0.join("config.yaml")).is_empty());
}

#[test]
fn reemplaza_el_esfuerzo_existente() {
    let text = "model: x\ndefaultModelThinking:\n  effort: low\nother: 1\n";
    assert_eq!(
        next_config_text(text, "high"),
        "model: x\ndefaultModelThinking:\n  effort: high\nother: 1\n"
    );
    let crlf = "defaultModelThinking:\r\n    effort: low\r\n";
    assert_eq!(
        next_config_text(crlf, "max"),
        "defaultModelThinking:\r\n    effort: max\r\n"
    );
}

#[test]
fn agrega_el_bloque_al_final_si_falta() {
    assert_eq!(
        next_config_text("model: x\n", "medium"),
        "model: x\ndefaultModelThinking:\n  effort: medium\n"
    );
    assert_eq!(
        next_config_text("model: x", "medium"),
        "model: x\ndefaultModelThinking:\n  effort: medium\n"
    );
    assert_eq!(
        next_config_text("model: x\r\n", "medium"),
        "model: x\r\ndefaultModelThinking:\r\n  effort: medium\r\n"
    );
    // El Python deja una línea vacía cuando el archivo está vacío.
    assert_eq!(
        next_config_text("", "low"),
        "\ndefaultModelThinking:\n  effort: low\n"
    );
}

#[test]
fn mismo_esfuerzo_no_cambia_el_texto() {
    let text = "defaultModelThinking:\n  effort: high\n";
    assert_eq!(next_config_text(text, "high"), text);
}

#[test]
fn esfuerzo_invalido_se_rechaza() {
    assert!(!set_mcode_effort("banana"));
    assert!(!set_mcode_effort("HIGH"));
}

#[test]
fn fija_el_esfuerzo_y_guarda_copia() {
    let dir = TempDir::new("mcode-escribe");
    let path = dir.0.join("config.yaml");
    let original = "provider:\n  mini:\n    models:\n      m: {}\n";
    std::fs::write(&path, original).expect("config");
    assert!(set_mcode_effort_at(&path, "max"));
    let updated = std::fs::read_to_string(&path).expect("config");
    assert!(updated.ends_with("defaultModelThinking:\n  effort: max\n"));
    assert_eq!(
        std::fs::read_to_string(dir.0.join("config.yaml.ezagent-bak")).expect("copia"),
        original
    );
    assert!(!dir.0.join("config.yaml.tmp").exists());
    // La segunda vez conserva la copia original.
    assert!(set_mcode_effort_at(&path, "low"));
    assert_eq!(
        std::fs::read_to_string(dir.0.join("config.yaml.ezagent-bak")).expect("copia"),
        original
    );
}

#[test]
fn si_el_config_no_existe_devuelve_falso() {
    let dir = TempDir::new("mcode-sin-config");
    assert!(!set_mcode_effort_at(&dir.0.join("config.yaml"), "high"));
}

#[test]
fn catalogo_de_codex_lee_solo_los_visibles() {
    let dir = TempDir::new("codex-catalogo");
    let data = serde_json::json!({
        "models": [
            {
                "slug": " gpt-5 ",
                "visibility": "list",
                "supported_reasoning_levels": [
                    {"effort": "low"},
                    "medium",
                    {"sin": "effort"}
                ]
            },
            {"slug": "oculto", "visibility": "hide", "supported_reasoning_levels": ["high"]},
            {"slug": "sin-niveles", "visibility": "list", "supported_reasoning_levels": []},
            "no-es-objeto"
        ]
    });
    std::fs::write(
        dir.0.join("models_cache.json"),
        serde_json::to_string(&data).expect("json"),
    )
    .expect("catálogo");
    let table = discover_codex_models_at(&dir.0);
    assert_eq!(table.len(), 1);
    assert_eq!(table["gpt-5"], serde_json::json!(["low", "medium", "None"]));
}

#[test]
fn catalogo_de_codex_tolera_errores() {
    let dir = TempDir::new("codex-errores");
    assert!(discover_codex_models_at(&dir.0).is_empty());
    std::fs::write(dir.0.join("models_cache.json"), b"{roto").expect("archivo");
    assert!(discover_codex_models_at(&dir.0).is_empty());
    std::fs::write(dir.0.join("models_cache.json"), b"[]").expect("archivo");
    assert!(discover_codex_models_at(&dir.0).is_empty());
}

#[test]
fn strip_jsonc_quita_comentarios_y_comas_finales() {
    let text = "{\n  // línea\n  \"a\": 1, /* bloque */\n  \"b\": \"// no es comentario\",\n  \"c\": [1, 2,],\n}\n";
    assert_eq!(
        strip_jsonc(text),
        "{\n  \n  \"a\": 1, \n  \"b\": \"// no es comentario\",\n  \"c\": [1, 2]\n}\n"
    );
    let parsed: Value = serde_json::from_str(&strip_jsonc(text)).expect("json válido");
    assert_eq!(parsed["c"], serde_json::json!([1, 2]));
    assert_eq!(strip_jsonc("/* sin cierre"), "");
    assert_eq!(strip_jsonc("\"a\\\\\\\"b\" // x"), "\"a\\\\\\\"b\" ");
}

#[test]
fn strip_jsonc_reproduce_la_coma_dentro_de_una_cadena() {
    // Quirk del Python: la limpieza de comas finales se aplica también al
    // interior de las cadenas.
    assert_eq!(strip_jsonc("{\"a\": \", }\"}"), "{\"a\": \" }\"}");
}

#[test]
fn modelo_por_defecto_desde_cadena() {
    assert_eq!(
        default_model_from("{\"model\": \"openai/gpt-5#high\"}").as_deref(),
        Some("openai/gpt-5")
    );
    assert_eq!(default_model_from("{\"model\": \"sin-proveedor\"}"), None);
    assert_eq!(default_model_from("{\"model\": 7}"), None);
    assert_eq!(default_model_from("no json"), None);
}

#[test]
fn modelo_por_defecto_desde_objeto() {
    assert_eq!(
        default_model_from("{\"model\": {\"providerID\": \"openai\", \"model\": \"gpt-5\"}}")
            .as_deref(),
        Some("openai/gpt-5")
    );
    assert_eq!(
        default_model_from("{\"model\": {\"providerID\": \"openai\"}}"),
        None
    );
    assert_eq!(
        default_model_from("{\"model\": {\"providerID\": \"\", \"model\": \"x\"}}"),
        None
    );
}
