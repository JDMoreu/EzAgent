# HD-0f — Corrección de la revisión de HD-0e: que el producto vea lo que escribe el supervisor

Revisión del agente principal sobre `work/HD-0d` (9108a2b, 22a364e, 3d22f61). Bien: el
límite solo con el agente parado, la pantalla quieta y frases completas en las últimas 15
líneas; la terminal no se cierra; `probar.sh` 15 casos en verde dentro y fuera de hc
(repetido por el elefante sobre una copia). Mutaciones del elefante: ignorar `working` →
falla el caso 15; mirar la pantalla entera → falla el 14; volver a contar «quota» → falla
el 12. **Pero quitar la ventana de pantalla quieta no rompe ninguna prueba.** Y hay fallos
nuevos que el producto ya no tolera:

1. **Carpeta equivocada.** El supervisor escribe en `…\EzAgent\subagents` (inglés); el
   producto (`Registry::from_env`) lee `%LOCALAPPDATA%\EzAgent\subagentes` (o
   `EZ_SUBAGENTS_DIR`). `ez tasks` y `ez watch` nunca verían a los hijos del supervisor.
2. **Forma del registro.** Escribe `"validation": {"by": null, "verdict": null, …}`; el
   producto define `validation` como `null` o un objeto con valores. Hasta HD-10 eso tumbaba
   `ez tasks` y dejaba ciego al vigilante; HD-10 lo tolera, pero el supervisor debe escribir
   la forma canónica: `"validation": null`.
3. **Acentos y símbolos.** `report_summary` (y todo Python del script) imprime en cp1252
   cuando la salida es una tubería. Comprobado por el elefante: un resumen con «→» sale
   **vacío** (UnicodeEncodeError tragado por `2>/dev/null`) y uno con «ó» sale en cp1252 y
   llega roto al registro. Los resúmenes de los informes casi siempre traen tildes.
4. **«Líneas con texto».** El producto (NV-1c) cuenta como línea con texto la que tiene al
   menos una letra o un dígito: los bordes del TUI (`┃`, `╹▀▀▀`) no cuentan. El supervisor
   usa `NF` (cualquier carácter no blanco). Usa el mismo criterio que el producto.

## Cambios (solo `supervise3.sh` y `pruebas/*`)
1. `SUBAGENTS_ROOT` por omisión = `${LOCALAPPDATA}/EzAgent/subagentes` (con `EZ_SUBAGENTS_DIR`
   mandando, como hoy).
2. `"validation": null` en todo registro nuevo.
3. `export PYTHONIOENCODING=utf-8 PYTHONUTF8=1` al principio del script.
4. `screen_tail`: las últimas 15 líneas que tengan al menos una letra o un dígito.
5. Pruebas nuevas en `probar.sh`:
   - **caso 16**, pantalla que cambia: el trabajador falso, `idle`, deja una frase completa
     de límite en sus últimas líneas pero cambia una línea (un contador) cada pocos segundos
     durante más de la ventana → **no** es límite. Comprueba tú la mutación: sin la ventana
     de pantalla quieta, el caso 16 debe fallar (pega la salida).
   - **caso 17**, pantalla real: `fake-hc` devuelve la captura real
     `rust-migration/briefs/NV-1c-pantalla-opencode.txt` (cópiala a `pruebas/` como dato)
     con `Free limit reached. Resets at 7:00 p.m.` añadida al final de la conversación, antes
     de las filas vacías, `idle` y quieta → límite detectado.
   - **caso 18**, acentos: un informe con `- Estado: completado — verificación → ok` deja en
     el registro exactamente ese resumen (léelo en UTF-8).
   - **producto lee al supervisor**: tras los casos, `"$EZ_BIN" tasks --parent <padre falso>
     --json` con `EZ_SUBAGENTS_DIR` = la carpeta de las pruebas → código 0, stdout es un array
     con los nombres de los casos y stderr **sin** «registro ilegible». `EZ_BIN` por omisión
     `C:/Users/JDMoreu/Documents/EzAgent/rust/target/debug/ez.exe` (ya compilado; **no
     compiles Rust**); si no existe, FALLO con el motivo, nunca saltar la prueba.
   - Ninguna prueba vieja se borra.

## Entrega
`bash -n` y `probar.sh` en verde dentro y fuera de hc, salidas literales, y la mutación del
caso 16. Commit en `work/HD-0d`, informe `rust-migration/reports/HD-0f-r1.md`. No lances
agentes reales ni compiles Rust.
