## Resultado
- Brief/tarea: IR-3c
- Estado: completado

## Cambios
- `rust/crates/ez-scan/src/usage/depleted.rs`: una entrada de `uso.json` sin `hasta` ya no queda vigente para siempre. Vence como los avisos sin hora de IR-2b: 24 h si el modelo termina en `-free`; en el resto, la ventana del agente en `planes.toml` o 5 h. Cuenta desde `visto_ms` y, si falta, desde la fecha de modificación de `uso.json`; sin ninguno de los dos, no cuenta.
- `rust/crates/ez-scan/src/usage/depleted.rs` (pruebas): sin `hasta` reciente y pasada la ventana, `*-free` a las 23 h y 25 h, fecha de modificación como respaldo y ventana tomada de `planes.toml`.

No se tocó ningún otro archivo de producción.

## Decisiones
- `# DECISION` en `active`: sin `hasta` ni momento de anotación no se puede saber la edad; como IR-2b con un aviso sin `observed_ms`, la entrada no cuenta.
- `# DECISION` en `window_hours`: para no cambiar la firma ni `usage.rs` (alcance de IR-3c), `depleted.rs` lee `planes.toml` directamente; es un archivo pequeño y el valor es el mismo que usa el informe.
- Una entrada sin `modelo` (agente entero) también vence por la ventana; la regla de 24 h es solo para modelos `*-free`, como en IR-2b.
- `hasta` con hora sigue mandando sobre todo: si está en el futuro cuenta y si pasó no cuenta, sin mirar la ventana.
- Si la ventana de `planes.toml` no es válida (cero, negativa o no finita), se usa la de 5 h.

## Verificación
Desde `rust/`:

```
$ cargo fmt --check -p ez-scan
fmt_exit=0

$ cargo clippy -p ez-scan --all-targets -- -D warnings
    Checking ez-scan v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.89s

$ cargo test -p ez-scan
ez-scan: test result: ok. 82 passed; 0 failed  (lib)
         test result: ok. 3 passed; 0 failed   (real_system)
         test result: ok. 1 passed; 0 failed   (usage_readers)
Total: 86 passed; 0 failed.
```

Las pruebas nuevas de `usage::depleted::tests` son:

```
sin_hasta_y_reciente_cuenta_pero_pasada_la_ventana_no ... ok
un_modelo_gratuito_sin_hasta_dura_24_horas ... ok
sin_hasta_ni_visto_usa_la_fecha_del_archivo ... ok
la_ventana_sin_hasta_sale_de_planes_toml ... ok
```

## Pendiente o riesgos
- El texto de `sources.state` para una entrada de agente sin `hasta` sigue diciendo «sin reinicio conocido»; ahora vence por la ventana, pero no se cambió el texto para no ampliar el alcance.
- En un modelo gratuito cuyo nombre no termine en `-free` se aplicaría la ventana normal; es la misma convención que usa IR-2b (`is_free`).
- No se lanzaron agentes reales; todo se probó con `uso.json` temporales y relojes fijos.
