# P-AGENT-B — Paridad de ez-agent, parte B (traspasos, tools_d, archivos y núcleo)

## Resultado
- Brief/tarea: P-AGENT-B
- Estado: completado

Los cinco dominios quedan al 100 %: 540/540 casos del oráculo pasan.

## Cambios
- `rust/crates/ez-agent/tests/handoff_parity.rs`: nuevo; 123 casos de `extract_answer`, `clean_tail`, `answer_or_tail` y `compose_handoff`.
- `rust/crates/ez-agent/tests/handoff_packet_parity.rs`: nuevo; 129 casos de `build_packet`, `outline`, `packet_text` y `has_work`, con dobles de `select`/`save`.
- `rust/crates/ez-agent/tests/tools_d_parity.rs`: nuevo; 82 casos de `load_tools_d`, `ToolSpec.handler` y `clean_environment` con un doble de `subprocess.run`.
- `rust/crates/ez-agent/tests/tools_fs_parity.rs`: nuevo; 120 casos con perfil temporal, Papelera y `os.startfile` registrados, reloj congelado, árbol de disco y diario.
- `rust/crates/ez-agent/tests/tools_core_parity.rs`: nuevo; 86 casos de `build_core_tools`, `get_datetime`, `list_dir` y `read_file`.
- `rust/crates/ez-agent/src/tools_d.rs`: `Runner` + `load_tools_d_with` para inyectar el doble del subproceso y `call_handler` para reproducir `tool.handler(args)` con argumentos que no son objeto.
- `rust/crates/ez-agent/src/tools_d/tests.rs`: el `RunContext` de las pruebas unitarias recibe el runner del sistema.
- `rust/crates/ez-agent/src/tools_core.rs`: `get_datetime_at` recibe el instante y la zona local inyectados; `get_datetime` sigue usando el reloj real.
- `rust/crates/ez-agent/src/tools_fs/util.rs`: trait `Trash` (+ `SystemTrash`), reloj inyectable para las copias `.bak`, y `FsContext` con ambos ganchos.
- `rust/crates/ez-agent/src/tools_fs.rs`: `build_fs_tools_with` con Papelera/reloj inyectables; `build_fs_tools` conserva la firma y usa los reales.
- `rust/crates/ez-agent/src/tools_fs/handlers.rs`: usa los ganchos y evalúa `kind` después del movimiento para reproducir el quirk de Python.
- `rust/crates/ez-agent/src/tools_fs/describe.rs`: pasa la marca de tiempo inyectada a `backup_path`.
- `rust/crates/ez-agent/src/journal.rs`: `with_hooks` (Papelera y reloj) y el traspaso del gancho a `undo_last`/pasos inversos. **Desviación de la lista de archivos del brief**, explicada en Decisiones.
- `rust-migration/python-quirks.md`: añadido Q-PAGENTB-01 al final.
- `rust-migration/reports/P-AGENT-B-r1.md`: este informe.

## Paridad por dominio
| Dominio | Casos | Al primer intento | Defectos Rust corregidos | Fallos restantes |
|---|---:|---:|---|---:|
| `handoff` | 123 | 123 | 0 | 0 |
| `handoff_packet` | 129 | 129 | 0 | 0 |
| `tools_d` | 82 | 82 | 0 | 0 |
| `tools_fs` | 120 | 117 | 1 (`transfer`: `kind` antes del movimiento) | 0 |
| `tools_core` | 86 | 86 | 0 | 0 |
| **Total** | **540** | **537** | **1** | **0** |

Notas de los «primer intento»: antes de comparar comportamiento hubo que montar
el arnés de cada dominio (inyección de subproceso, reloj, Papelera y cwd). Los
fallos intermedios de `tools_fs` (`ts` del diario, orden del árbol,
`read-outside-workspace`) fueron del arnés, no del módulo Rust.

### Defecto corregido
- `tools_fs/handlers.rs::transfer` calculaba `kind_of(&src)` **antes** del
  movimiento. El Python lo calcula después (`tools_fs.py:426-431`), cuando el
  origen ya no existe, y por eso `move-folder`, `move-folder-empty` y
  `move-folder-overwrite` esperan `"kind": "file"`. Se movió el cálculo tras
  `move_into_place`/`record_move`; `copy_path` no cambia (`"folder"`, el origen
  sigue existiendo). Anotado como quirk Q-PAGENTB-01.

## Decisiones
- **Inyección de subprocesos en `tools_d`** (`Runner`, `load_tools_d_with`): el
  oráculo sustituye `subprocess.run`; sin gancho, 40 casos lanzarían `echo`/`code`
  reales o fallarían por no existir. `load_tools_d` mantiene su firma.
- **`tools_d::call_handler`**: reproduce `tool.handler(args)` para el caso 0040
  (`args` es la cadena `"texto"`), que en Python devuelve «Los argumentos deben
  ser un objeto.». La firma Rust del manejador exige un mapa.
- **Inyección de reloj en `tools_core`** (`get_datetime_at`): el oráculo congela
  `datetime` en `2025-01-01T12:34:56Z` (y tres instantes más para `summerNN`);
  sin gancho, `get_datetime` no es comparable. Producción sigue con `Local::now()`.
- **Inyección de Papelera y reloj en `tools_fs`** (`Trash`, `Clock`,
  `build_fs_tools_with`): la Papelera real no se usa en pruebas y las copias
  `.bak` deben ser estables (`20250101-000000`). `build_fs_tools` conserva firma.
- **`journal.rs::with_hooks` (desviación)**: el oráculo también sustituye
  `journal._send_to_trash` y `journal.datetime`; los casos de `undo_last` de
  `tools_fs` traspasan archivos y escriben `ts`/`undone_at`. Sin el gancho, las
  pruebas mandarían archivos temporales a la Papelera real y el diario no sería
  determinista. El cambio es aditivo y `Journal::new` sigue usando Papelera y
  reloj reales. No toqué ningún otro archivo fuera de la lista.
- **Los tests llaman `tool.handler` directamente**, como el generador del
  oráculo, en vez de `ToolRegistry::run`: la validación de esquema del registro
  cambiaría resultados (`undo_last` con `n=0`, `get_datetime` con `timezone=5`).
- **`select='raise'` se modela como `None`**: el `except Exception` de Python
  convierte la excepción en «sin modelo»; la firma Rust no puede lanzar desde
  `SelectFn` y el resultado visible es el mismo. Los dos casos de `save` que
  lanza se reproducen con pánico + `catch_unwind`.
- **Errores de tipo del oráculo** (`screen=None`, `text=7`, `packet=None`,
  `has_work(None)`): se comprueban en el borde de la prueba (la firma Rust no
  admite esos valores), igual que hace `speech_text_parity.rs`.
- **`ts`/`undone_at` del diario**: se inyecta el reloj ISO congelado
  (`2025-01-01T00:00:00+00:00`) en vez de normalizar la comparación.

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-agent --all-targets -- -D warnings`: ok.
- `cargo test -p ez-agent`: 162 pruebas unitarias + 6 suites de integración, todas en verde.
  - `handoff_parity`: 1 test, 123 casos.
  - `handoff_packet_parity`: 1 test, 129 casos.
  - `tools_d_parity`: 1 test, 82 casos.
  - `tools_fs_parity`: 1 test, 120 casos.
  - `tools_core_parity`: 1 test, 86 casos.
  - `policy_parity` (existente): 1 test.
- No se ejecutaron pruebas de Python: `src/` y `tests/` son de solo lectura y no
  se modificaron.
- Límites: módulos `.rs` tocados ≤ 565 líneas (`tools_d.rs`, contado tras
  `cargo fmt`); tests ≤ 455 líneas. Sin `todo!`, `unimplemented!`, `#[ignore]`,
  `TODO`, `println!`, `remove_dir_all`, `unsafe`, `.unwrap()` fuera de `tests/`
  ni `cmd /c`.

## Fixtures consumidos
| Fixture | Casos | Hash (referencia) |
|---|---:|---|
| `parity/fixtures/handoff.jsonl` | 123 | leído, no modificado |
| `parity/fixtures/handoff_packet.jsonl` | 129 | leído, no modificado |
| `parity/fixtures/tools_d.jsonl` | 82 | leído, no modificado |
| `parity/fixtures/tools_fs.jsonl` | 120 | leído, no modificado |
| `parity/fixtures/tools_core.jsonl` | 86 | leído, no modificado |

## Pruebas portadas (filas de `parity/test-map.csv`)
- `tests/test_handoff.py`: 24 filas (`test_answer_or_tail_usa_la_respuesta_y_si_no_la_cola`,
  `test_claude_con_herramientas_devuelve_la_conclusion_sin_el_pie`,
  `test_clean_tail_quita_marcos_y_pie_y_se_queda_con_el_final`,
  `test_compose_*` (5), `test_con_herramientas_sin_marca_hc_hace_falta_la_tarea_enviada`,
  `test_el_razonamiento_del_modelo_no_se_cuela_en_la_respuesta`,
  `test_el_texto_enviado_se_reconoce_aunque_la_pantalla_lo_parta_en_lineas`,
  `test_el_ultimo_intercambio_gana_si_hay_varios`,
  `test_extrae_solo_la_respuesta_en_cada_agente[*]` (4),
  `test_la_respuesta_no_incluye_cabecera_eco_ni_pie[*]` (4),
  `test_la_respuesta_se_recorta_al_maximo`,
  `test_la_salida_del_agente_es_un_dato_no_se_interpreta`,
  `test_los_parrafos_separados_por_lineas_en_blanco_se_conservan`,
  `test_pantalla_vacia_o_irreconocible_devuelve_vacio`).
- `tests/test_handoff_packet.py`: 16 filas (`test_con_trabajo_solo_pasa_la_conclusion`,
  `test_el_limite_de_corto_es_inclusivo`,
  `test_el_registro_largo_se_guarda_y_el_corto_no`,
  `test_el_selector_del_modelo_solo_mueve_la_linea_de_inicio`,
  `test_hay_trabajo_cuando_aparecen_marcas_de_herramientas`,
  `test_outline_toma_primer_parrafo_y_titulos_sin_duplicados`,
  `test_outline_vacio`, `test_packet_text_agrega_la_nota_con_la_ruta`,
  `test_si_guardar_falla_no_se_inventa_una_ruta`,
  `test_sin_modelo_se_limita_la_conclusion_por_lineas_enteras`,
  `test_texto_vacio_da_un_paquete_corto_vacio`,
  `test_un_documento_muy_largo_se_guarda_y_se_pasa_un_indice`,
  `test_un_documento_muy_largo_sin_guardador_se_pasa_entero`,
  `test_un_selector_hostil_o_roto_se_ignora`,
  `test_un_texto_largo_sin_trabajo_se_pasa_entero_hasta_el_limite_de_archivo`,
  `test_una_respuesta_corta_sin_trabajo_se_pasa_tal_cual`).
- `tools_d`, `tools_fs` y `tools_core` no tienen filas propias en
  `parity/test-map.csv`; su cobertura es la de los fixtures (288 casos).

## Pendiente o riesgos
- Ningún caso falla. El único riesgo abierto es la desviación en
  `journal.rs::with_hooks`: si otra tarea de paridad del diario (F3-04) toca ese
  archivo, debe conservar la firma `with_hooks(directory, trash, clock)` y el
  comportamiento de `new` (Papelera y reloj reales).
- `list_dir/023` depende del orden de `read_dir` en NTFS (los 200 primeros de
  205 archivos): pasó aquí, pero en un volumen con otro orden podría variar; el
  fixture no es determinista por diseño.
- `read_file` usa `from_utf8_lossy`; Python `errors="replace"` puede diferir con
  secuencias UTF-8 inválidas (ningún caso del fixture las usa).
- La etiqueta de zona local de `get_datetime` sigue siendo `UTC±hh:mm` (decisión
  heredada de F3-07); el oráculo usa el nombre `tzname()` de Windows. No afecta
  a los fixtures porque congelan el reloj en UTC.
