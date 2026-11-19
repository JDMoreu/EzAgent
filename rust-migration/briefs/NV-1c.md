# NV-1c — Corrección de la revisión de NV-1b: el límite real de OpenCode no se vería

Revisión del agente principal sobre `work/NV-1` (2414226 + 0be0345). Bien: frases
completas de §3, `working` excluido, recorte compartido con la reanudación, pruebas de
palabras sueltas y de la frase alta. **Mal**, dos cosas:

1. **`last_lines` cuenta líneas en blanco.** La pantalla real de OpenCode que devuelve
   `hc read --lines 0` deja muchas filas vacías entre la conversación y la caja de
   entrada. En `rust-migration/briefs/NV-1c-pantalla-opencode.txt` (captura real de tu
   propia terminal, 121 líneas) las 15 últimas son 7 vacías + la caja (`┃`, `Build auto ·
   …`, `╹▀▀▀`, la ruta) + `[hc] incomplete…`: **ninguna línea de la conversación**. El caso
   real de IR-1 (límite al arrancar, mensaje arriba y la pantalla vacía debajo) **no se
   detectaría nunca**. Las pruebas pasan porque las pantallas falsas no tienen ese relleno.
2. **«La primera lectura cuenta como estable»** contradice §3 («la pantalla no cambió
   desde la vuelta anterior»): al reiniciar el vigilante, un hijo `idle` cuyo último
   mensaje cite una frase de límite queda `bloqueado` en la primera vuelta. El supervisor
   (HD-0e) ya exige dos lecturas iguales; el producto debe hacer lo mismo.

## Cambios (mismo alcance que NV-1/NV-1b)
1. Las «últimas 15 líneas» son las últimas 15 **con texto**: líneas con al menos una letra
   o un dígito (`char::is_alphanumeric`). Las vacías y las que solo tienen bordes o
   espacios (`┃`, `╹▀▀▀`) no cuentan. Aplica igual al bloqueo, a la hora de reinicio y a
   la reanudación. Si `SCREEN_LIMIT` (4000 caracteres) recorta antes, revisa que con la
   pantalla real (filas de hasta ~460 caracteres) siga habiendo 15 líneas con texto; si no,
   recorta por líneas en vez de por caracteres. `# DECISION` si cambias el límite.
2. La primera lectura de un hijo **no** cuenta como estable: hace falta una segunda vuelta
   con la misma pantalla. Ajusta la prueba original de NV-1 a dos vueltas.
3. Pruebas nuevas, con un fixture construido a partir de la captura real (cópiala dentro
   del crate como fixture de pruebas; no la leas desde `briefs/`):
   - la captura con una línea `Free limit reached. Resets at 7:00 p.m.` añadida al final de
     la conversación (antes de las filas vacías), `idle`, dos vueltas iguales → `bloqueado`
     con la hora 19:00 en `uso.json`;
   - lo mismo en la **primera** vuelta → todavía no bloquea;
   - la misma frase con 20 líneas con texto debajo → no bloquea;
   - la captura sin frase, `idle`, estable → no bloquea.
4. Ninguna prueba vieja se borra; si una cambia, explica por qué en el informe.

## Entrega
fmt, clippy y test de `ez-herdr` en verde, salida literal en
`rust-migration/reports/NV-1c-r1.md`. Commit en `work/NV-1`. No toques
`docs/designs/novedades-subagentes.md` (lo cambió el revisor) ni ejecutes agentes reales.
