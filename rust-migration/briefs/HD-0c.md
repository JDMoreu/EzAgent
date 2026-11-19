# HD-0c — Correcciones de la revisión de HD-0b (comprobadas en vivo por el agente principal)

HD-0b quedó bien con agentes falsos (5 casos en verde, avisos al padre falso). Pero el
agente principal probó la envoltura con el **OpenCode real, sin encargo** (abrir la
interfaz no llama a ningún modelo) y falla. Mismo alcance de archivos que HD-0.

## C0-4. `env.exe opencode` ejecuta el guion de bash de npm, no OpenCode
`env.exe` resuelve `opencode` a `C:\Users\JDMoreu\AppData\Local\pi-node\current\opencode`
(guion de bash de npm) y sale con 127 (`sed: command not found`, `.../bin/opencode.exe: No
such file or directory`). Con la ruta del ejecutable nativo funciona. Resuelve el
programa real antes de lanzar: `EZ_OPENCODE_BIN` si está; si no, a partir de
`where.exe opencode.cmd`, el `node_modules/@opencode/cli/bin/opencode.exe` de esa carpeta;
si no existe, `opencode.cmd`. Comprueba con `"<bin>" --version` (no gasta nada) y, si
falla, dilo y usa el modo `run`.

## C0-5. OpenCode solo acepta ids de sesión que empiezan por «ses»
`--session hd-probe-s1` → `Error: Expected a string starting with "ses"` y código 1. Con
`ses_hdprobe01` abre normal. Usa `ses_<id en minúsculas, solo a-z0-9>s1` (p. ej.
`ses_hd0cs1`) y su prueba.

## C0-3. Un `hc get` fallido no es «terminado»
Durante un reinicio del núcleo de hc, `field` devolvió vacío y el bucle dio por terminado
un intento con la terminal viva (luego `name_taken`). Un `hc get` fallido o vacío es
«desconocido»: reintenta (hasta 2 min) antes de decidir; y nunca lances un intento nuevo
mientras una terminal con ese nombre siga viva.

## Prueba de humo real, sin encargo (permitida solo así)
Añade a `pruebas/` un caso **opt-in** (`EZ_HUMO_REAL=1`) que abre el OpenCode real con la
misma envoltura y `--session ses_…`, **sin `--prompt`**, espera hasta 15 s a que `hc`
reporte `idle`, lo cierra (`hc kill` + `hc rm`) y comprueba que no salió con error. No
escribe nada en la interfaz ni envía mensajes. Ejecútalo una vez y pega la salida.

## Entrega
`bash -n` y `probar.sh` en verde (y el humo real), con la salida pegada. Informe en
`rust-migration/reports/HD-0c-r1.md`.
