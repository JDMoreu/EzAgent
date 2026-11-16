# TMAP-1 — Mapa de pruebas Python → Rust, grupo 1 (313 pruebas)

## Resultado
- Brief/tarea: TMAP-1
- Estado: completado

## Cambios
- `parity/test-map/TMAP-1.csv`: 313 filas (una por prueba Python del grupo) con `python_test_id,rust_crate,rust_test_path,estado,nota`; sustituye las suposiciones del CSV global y corrige el crate cuando la conducta vive en otro.
- `rust/crates/ez-agent/src/confirm.rs`: prueba `a_stale_arm_does_not_start_the_deadline` (un `arm` con otro token no abre el plazo).
- `rust/crates/ez-agent/src/registry/tests.rs`: pruebas de normalización de las nueve formas de retorno, re-registro que reemplaza, registro concurrente y validación `boolean`/`number`.
- `rust/crates/ez-agent/tests/tmap_web_gaps.rs`: hosts decimal/hex/octal, DNS mixto (pública + privada) y redirección con credenciales; reutiliza los dobles de `tools_web_parity/fakes.rs`.
- `rust/crates/ez-chat/tests/tmap_chat_gaps.rs`: `seq` persistido, orden con reloj hacia atrás, archivo antiguo sin `seq` y turnos de voz con su `source`.
- `rust/crates/ez-chat/src/speech.rs`: la prueba de recorte largo ahora comprueba también el final en frase (`resumen.`) y que conserva el principio.
- `rust/crates/ez-hud/src/win/drop/ui/tests.rs`: `bar_hit_testing_follows_scale` (la forma sigue siendo la forma con `bar_scale` 2.0).
- `rust/crates/ez-app/tests/tmap_instant_settings.rs`: `INSTANT_SETTINGS` de la app == `INSTANT_KEYS` del HUD, con `ui_scale` y `show_advanced`.
- `rust/crates/ez-swarm/src/store/tests/gaps.rs`: `claim` concurrente, partes que cierran en silencio y liberan reservas, reapertura con partes y reapertura de la base con miembros.
- `rust/crates/ez-swarm/src/store/tests.rs` y `src/supervisor/tests.rs`: `mod gaps;` para los submódulos nuevos.
- `rust/crates/ez-swarm/src/supervisor/tests/gaps.rs`: avisos de atención una sola vez y reintento de la línea de inicio con tope `MAX_NUDGES`.

## Decisiones
- 4 filas de `Ctrl + rueda` (incluida la rueda fuera de la barra) quedan `no_aplica`: la prueba de Python inyecta `GetAsyncKeyState` con `monkeypatch`; en Rust `ctrl_down()` consulta el teclado real y no hay costura, y tocar producción queda fuera del alcance del brief.
- `test_register_solo_acepta_toolspec` queda `no_aplica`: `register(&self, ToolSpec)` lo impide en compilación; el `TypeError` del Python no tiene equivalente en Rust.
- `test_los_proxies_del_entorno_se_ignoran` queda `no_aplica`: el `httpx.MockTransport` inyectado no pasa por reqwest; el equivalente real es `.no_proxy()` en `real_exchange`, sin costura en la prueba.
- `test_entries_use_the_per_session_index_and_do_not_leak_the_cache` → `paridad` con `chat_store/delete-session`: `_by_session` es privado y `entries()` devuelve una copia; la conducta observable (borrar una sesión no toca a las demás) sí está cubierta.
- Las pruebas de normalización de `registry` comparten una sola prueba Rust (`normalizes_every_python_return_shape`) porque las nueve formas se distinguen solo por el valor devuelto; igual criterio para las dos filas de tipos en `validates_boolean_and_number_types`.
- `test_card_renders_at_any_size` en tamaño 1200x800 no tiene cuadro dorado propio (los `@1200` del fixture son 560x400/760x620); se deja `paridad` con la nota del tamaño realmente pintado, en vez de duplicar quince renderizados.

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-agent -p ez-chat -p ez-hud -p ez-swarm -p ez-app --all-targets -- -D warnings`: ok.
- `cargo test -p ez-agent`: 256 pruebas de la lib + integración, 0 fallos (incluye los 3 casos nuevos de `tmap_web_gaps` y las 4 de `registry`).
- `cargo test -p ez-chat`: 79 de la lib + integración, 0 fallos (incluye `tmap_chat_gaps`).
- `cargo test -p ez-swarm`: 126 de la lib + integración, 0 fallos (incluye `store::tests::gaps` y `supervisor::tests::gaps`).
- `cargo test -p ez-hud`: suite completa, 0 fallos (incluye `bar_hit_testing_follows_scale`).
- `cargo test -p ez-app --test tmap_instant_settings`: 1, 0 fallos.
- Fixtures consumidos (filas del mapa / casos del archivo): `confirm.jsonl` 6/37, `audit.jsonl` 2/20, `redact.jsonl` 9/53, `chat_store.jsonl` 13/85, `orchestration.jsonl` 32/46, `registry.jsonl` 26/68, `tools_web.jsonl` 17/240, `hud_menu.jsonl` 20/323, `hud_settings.jsonl` 1/1175, `swarm.jsonl` 1/644.
- Pruebas portadas: las 313 filas del CSV; la lista completa de `python_test_id` es el propio archivo. Conteo por estado: `portada` 154, `paridad` 128, `nueva` 25, `no_aplica` 6. Por archivo: audit_confirm 23 (17/5/1), audit_store 6 (3/0/3), chat_session 29 (23/5/1), chat_store 14 (9/5/0), orchestration 32 (0/32/0), registry 39 (3/26/9+1), settings_window 47 (19/22/2+4), swarm_store 19 (14/1/4), swarm_supervisor 10 (8/0/2), web_security 94 (73/17/3+1).

## Pendiente o riesgos
- Hallazgos: no apareció ninguna diferencia de conducta Python↔Rust que obligara a `#[ignore]`; las 25 pruebas `nueva` pasan tal cual.
- Cobertura parcial consciente: `test_card_renders_at_any_size` en 1200x800 y `test_bar_scale_does_not_touch_settings_scale` se apoyan en la fórmula común ya probada, no en una prueba que mida exactamente ese caso; anotado en cada fila.
- `test_un_token_incorrecto_no_resuelve` comprueba el token ajeno; la longitud de 32 caracteres se cumple en `uuid_hex` pero no se asevera en la prueba Rust.
- `bar_wheel` sigue sin prueba directa (ver Decisiones); si en el futuro se añade una costura de teclado, esas cuatro filas pueden pasar a `portada`.

## Desviaciones del brief
- Ninguna en archivos: solo se tocaron los permitidos (`parity/test-map/TMAP-1.csv`, pruebas en `rust/crates/**` y este informe). No se modificó `Cargo.toml` ni código de producción salvo añadir `mod gaps;` en dos módulos de pruebas existentes.
- El CSV global `parity/test-map.csv` no se tocó.
