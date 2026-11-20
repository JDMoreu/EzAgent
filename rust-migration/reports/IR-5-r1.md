## Resultado
- Brief/tarea: IR-5
- Estado: completado

## Cambios
- `rust/crates/ez-scan/src/recommend/modelos.rs`: el lector reconstruye los nombres con punto que TOML anida (`[deepseek-v4.1-flash]` → `deepseek-v4` + `1-flash` → `deepseek-v4.1-flash`, recursivo para varios puntos), acepta la forma citada, y si no queda ninguna entrada válida avisa primero que usa los valores de fábrica y después el motivo de cada entrada saltada. La cabecera ya muestra la forma citada.
- `rust/crates/ez-scan/src/real.rs`: `herdr()` y `run()` usan `VoiceConfig::load(None)` (el `voice.json` cargado, el mismo de `ez delegate`) en vez de `VoiceConfig::new()`; si el archivo no se puede leer se avisa y se cae a fábrica. La medición de subagentes se extrajo a `subagents_with(&Herdr, …)` para probarla con un runtime falso.
- `rust/crates/ez-scan/src/entorno.rs`: `entorno.json` gana `usage` con un resumen por agente (estado, plan, ventana, reinicio y fuente), sin tokens ni datos de conversaciones.
- `rust/crates/ez-ctl/src/local/choice.rs`: con `--agent` y `--model` explícitos se respeta el modelo y solo se completa el esfuerzo con los niveles del agente (el más cercano al del papel); si el agente no declara niveles, el aviso habla solo del esfuerzo. Ya no se usa la recomendación para un modelo que la persona dio.
- `rust/crates/ez-ctl/tests/scan_init.rs`: pruebas de carpetas falsas para el punto sin comillas (la preferencia se respeta) y para la sección de uso de `entorno.json`.
- Sin cambios en `ez-scan/src/lib.rs` ni en `ez-ctl/src/scan_cmds.rs`: la sección de uso se arma dentro de `entorno::build`, que ya carga la configuración y las carpetas de uso del proceso.

## Decisiones
- `// DECISION` en `choice.rs::effort_rank`: el orden de niveles y el desempate repiten los de `ez_scan::recommend` (archivo fuera del alcance de IR-5); si cambian allí, hay que cambiarlos aquí.
- Con un agente que no declara niveles de esfuerzo (por ejemplo Qwen Code), no se inventa uno: `validate_choice` lo rechazaría. Se deja sin esfuerzo y el único aviso dice «No pude completar el esfuerzo…»; nunca se menciona el modelo.
- Prueba de la guarda con `Herdr::with_runtime(cfg, FakeHc)`: así se comprueba, sin procesos ni `hc` instalado, que se ejecuta el `hc_path` de la configuración con umbrales absurdos y que `capacity::check` devuelve el texto «Recursos justos:» (la traducción a código 4 ya está cubierta en `local::choice::tests`). No se añadió un binario `hc` falso.
- La prueba de `voice.json` → `hc_path` fija `APPDATA` a una carpeta temporal y lo restaura; ninguna prueba de `ez-scan` lee la configuración real.
- `ez init` no propone `modelos.toml` (solo `planes.toml`), así que la parte de «lo que proponga `ez init`» no tiene texto de modelos que citar; el ejemplo de la cabecera sí quedó con la forma citada.
- Se actualiza la sección de uso también para agentes no instalados (estado `desconocido`), porque es el mismo informe de `ez usage`.

## Verificación
Desde `rust/`, con `CARGO_BUILD_JOBS=2` y un `cargo` a la vez:

```
$ cargo fmt --check -p ez-scan -p ez-ctl
fmt=0

$ cargo clippy -p ez-scan --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.47s

$ cargo clippy -p ez-ctl --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3m 45s

$ cargo test -p ez-scan
test result: ok. 88 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out  (lib)
test result: ok. 3 passed; 0 failed  (real_system)
test result: ok. 1 passed; 0 failed  (usage_readers)
Total: 92 passed; 0 failed.

$ cargo test -p ez-ctl
test result: ok. 22 passed; 0 failed  (lib)
test result: ok. 0 passed; 0 failed   (bin ez)
test result: ok. 7 passed; 0 failed   (cli_errors)
test result: ok. 10 passed; 0 failed  (delegate_local)
test result: ok. 17 passed; 0 failed  (local_state)
test result: ok. 6 passed; 0 failed   (roundtrip)
test result: ok. 4 passed; 0 failed   (scan_init)
test result: ok. 7 passed; 0 failed   (server_errors)
test result: ok. 2 passed; 0 failed   (waits)
Total: 75 passed; 0 failed.
```

`cargo run -q -p ez-ctl --example e2e_inventory` (código 0), líneas finales:

```text
OK     `ez recommend` no descarta OpenCode por el modelo gratuito  opencode · opencode-go/deepseek-v4.1-flash
OK     no quedan terminales de la prueba en hc                   []
OK     la carpeta de la prueba se borra                          C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-17864-21300
86/86 OK
```

Pruebas nuevas de las reproducciones de IR-4:
- `modelos.rs::un_nombre_con_punto_sin_comillas_se_reconstruye` (y `la_forma_citada_da_el_mismo_nombre`, `sin_entradas_validas_avisa_de_la_fabrica_y_por_que`), más `scan_init.rs::recommend_respeta_un_modelo_con_punto_sin_comillas` con `APPDATA`/`PATH` falsos.
- `real.rs::la_guarda_rechaza_con_el_hc_configurado_y_umbrales_absurdos`: `hc_path` absoluto, runtime falso (sin depender del PATH) y umbrales 999 999 999 → «Recursos justos: …». `real.rs::voice_json_manda_sobre_el_hc_de_fabrica` cubre la carga del archivo.
- `choice.rs::con_modelo_explicito_sin_esfuerzo_solo_completa_el_esfuerzo` y `sin_niveles_de_esfuerzo_el_aviso_habla_solo_del_esfuerzo`.
- `entorno.rs::el_resumen_de_uso_trae_estado_plan_ventana_reinicio_y_fuente` y `scan_init.rs::init_escribe_entorno_con_los_agentes_falsos` (sección `usage`, sin `tokens`).

## Pendiente o riesgos
- No se añadió un binario `hc` falso para ejecutar el `ez delegate` completo hasta el código 4: la guarda se prueba en `capacity::check` con el runtime falso y el mapeo a código 4 en `local::choice::tests`. Queda cubierto sin depender de que `hc` esté instalado.
- `real.rs` lee `voice.json` en cada medición (una vez por `herdr()`), no una sola vez por proceso; es coherente con `ez delegate` y evita cachear configuración entre llamadas.
- `EZ_HC_PATH` no se consulta en `ez-scan` (sí en `local::state::Context` para el vigilante); el brief pedía la configuración cargada y eso es lo que se usa.
- El aviso de fábrica de `modelos.toml` sale por la misma vía que los demás (stderr en `ez recommend`, `warnings` en `ez delegate`); `ez init` no lo lee.
