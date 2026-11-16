# F7-06 — Prueba en vivo de EzAgent (versión Rust)

Esta es la lista que haces tú, con hardware y servicios reales, antes de dar por
buena la versión Rust. No hace falta que sea una sesión perfecta: lo importante
es que anotes lo que observas y que, si algo falla, quede el registro.

- Versión que pruebas: **0.2.0** (anota el commit o la fecha de compilación).
- Máquina y Windows:
- Micrófono y altavoces:
- Proveedor LLM y modelo:
- Fecha:

**Cómo usar la lista.** Marca cada casilla al terminarla. En cada bloque,
«Qué debes ver» es lo normal y «Si falla» es lo que hay que recoger. No borres
los registros: viven en `%APPDATA%\EzAgent\logs\` (`engine.log`, `audit.jsonl` y
`latency.jsonl`).

## 0. Antes de empezar

- [ ] Windows 11 de 64 bits, con el micrófono y los altavoces que uses a diario.
- [ ] Herdr instalado y abierto, con **Codex**, **OpenCode** y **Claude Code**
      instalados e iniciados sesión al menos una vez.
- [ ] Una clave de API válida del proveedor que vayas a probar.
- [ ] Una carpeta de pruebas vacía, por ejemplo
      `C:\Users\<tu usuario>\Documents\EzAgent-pruebas\`, y dentro
      `autonomo-a`, `autonomo-b` y `enjambre`.
- [ ] Una imagen pequeña local (PNG o JPG) para el chat, por ejemplo
      `C:\Users\<tu usuario>\Pictures\prueba.png`.
- [ ] `whisper-server` y su modelo instalados en `%LOCALAPPDATA%\EzAgent\whisper`
      (si no, se descargan una vez con `uv run python -m ezagent.core.downloads
      --all` desde el repositorio).

## 1. Arranque

### Desde el repositorio

- [ ] Cierra cualquier EzAgent anterior y ejecuta `EzAgent.bat`; elige
      **Iniciar mensajero de voz**.
- [ ] Qué debes ver: la barra aparece abajo al centro, el icono de la bandeja
      aparece y el registro dice que empieza a escuchar. La primera vez puede
      tardar unos segundos mientras arranca whisper.
- [ ] Si falla: anota el mensaje de la consola, revisa
      `%APPDATA%\EzAgent\logs\engine.log` y comprueba que
      `%LOCALAPPDATA%\EzAgent\whisper\whisper-server.exe` existe.

### Instalado, en otra cuenta o en otro equipo si puedes

- [ ] Consigue `dist\EzAgent-Setup-0.2.0.exe` (el instalador ya compilado o
      generado con `packaging\build-rust.ps1`) y ejecútalo. No debe pedir
      administrador.
- [ ] Abre EzAgent desde el menú Inicio. Si Windows muestra SmartScreen, elige
      **Más información → Ejecutar de todas formas** (el ejecutable no está
      firmado).
- [ ] Qué debes ver: la barra, la bandeja y la ventana de Ajustes funcionan
      igual; los datos se crean en `%APPDATA%\EzAgent`.
- [ ] Comprueba que la primera vez descarga Kokoro y Smart Turn (con su hash) y
      que whisper sigue disponible en `%LOCALAPPDATA%\EzAgent\whisper`.
- [ ] Si falla: anota si faltaba una DLL, el modelo o la conexión, y guarda el
      `engine.log` de esa cuenta.

## 2. Micrófono real

- [ ] Abre Ajustes → Escucha y elige tu micrófono. Habla y mira el medidor.
- [ ] Qué debes ver: el medidor se mueve con tu voz y se queda bajo en silencio;
      la barra pasa a «te escucha» y vuelve a «en espera» al terminar la frase.
- [ ] Haz una pregunta corta («¿qué hora es?») y espera la respuesta hablada.
- [ ] Qué debes ver: la transcripción es razonable, la respuesta llega por voz y
      la barra pasa por «piensa» y «habla».
- [ ] Si falla: anota el texto que entendió (o si no transcribió nada), el
      micrófono elegido y las últimas líneas del `engine.log`.

## 3. Interrupción por voz

- [ ] Pide una respuesta larga: «Cuéntame en un párrafo qué hace este proyecto.»
- [ ] Mientras habla, interrúmpela con voz normal: «Espera, para.»
- [ ] Qué debes ver: la voz se corta en menos de un segundo, la barra vuelve a
      escuchar y tu frase se atiende. Un clic o un golpe en la mesa no debe
      cortarla.
- [ ] Prueba una segunda vez con el volumen normal de la habitación (sin gritar).
- [ ] Si falla: anota si hizo falta gritar, si se cortó sola con ruido de fondo
      o si no se cortó nunca. Adjunta las líneas `Barge-in:` del `engine.log`.
      La primera frase tras arrancar es más conservadora durante 0,6 s: es
      normal.

## 4. Nombres dictados: «¿Te refieres a…?»

- [ ] En Herdr, abre un espacio llamado `TEST` (o usa uno que exista) con una
      terminal.
- [ ] Pide: «Lee la terminal TES» (di «TES», no «TEST»).
- [ ] Qué debes ver: EzAgent pregunta «¿Te refieres a la terminal «TEST»?» antes
      de leer nada. Responde «sí» y comprueba que lee esa terminal; repite y
      responde «no»: debe listar las terminales abiertas sin tocar nada.
- [ ] Prueba un nombre claramente distinto («XYZ»): debe decir que no lo
      encontró y listar los nombres vivos.
- [ ] Si falla: anota el nombre dicho, el que entendió y la respuesta. Adjunta
      la línea del `engine.log` de `hc ls` o del resolvedor.

## 5. LLM real y herramientas

- [ ] Pregunta la hora y la fecha.
- [ ] Pide leer un archivo pequeño de la carpeta de pruebas.
- [ ] Pide una búsqueda web sencilla («busca quién escribió el Quijote»).
- [ ] Pide recordar una preferencia («recuerda que prefiero respuestas cortas») y
      confirma con «sí» cuando pregunte.
- [ ] Qué debes ver: respuestas coherentes, acuses cortos antes de una
      herramienta, confirmación cuando corresponda y ninguna clave en el log.
- [ ] Si falla: anota el proveedor y el modelo, el código de error que se dice
      por voz y el detalle del `engine.log` (nunca copies una clave).

## 6. Herdr con Codex, OpenCode y Claude

Haz una tarea pequeña por agente, siempre en la carpeta de pruebas.

- [ ] **Codex:** «Abre Codex en el espacio `pruebas`, en la carpeta
      `...\EzAgent-pruebas\enjambre`, y pídele que cree `nota-codex.txt` con la
      fecha. Avísame cuando termine.»
- [ ] Qué debes ver: se abre una pestaña visible de Herdr, arranca Codex, fija
      modelo/esfuerzo si se lo pediste y pega la tarea. La barra piensa y luego
      avisa por voz con la tarea concreta.
- [ ] **OpenCode:** «Abre OpenCode en el mismo espacio y pídele que lea
      `nota-codex.txt` y escriba un resumen en `resumen-opencode.txt`. Vigílalo
      y avísame.»
- [ ] **Claude Code:** «Abre Claude Code con Sonnet y esfuerzo low; pídele que
      revise `resumen-opencode.txt` y diga si es correcto. Avísame al terminar.»
- [ ] Deja el puntero sobre la barra y comprueba la **vista rápida**: aparecen
      las terminales vigiladas y su estado. Hablar o escribir debe darlas por
      vistas.
- [ ] Prueba «lee la terminal de Codex» y «enfoca la terminal de OpenCode».
- [ ] Encadena: «Pásale la salida de Codex a OpenCode para que la comente.»
- [ ] Qué debes ver: cada cambio de etapa se dice sin reabrir la escucha y el
      final nombra el flujo completo.
- [ ] Si falla: anota en qué paso se quedó (no abrió, no pegó, no avisó, no
      encadenó), la terminal y las líneas de `delegate_to_agent` del log. Si un
      diálogo de permiso se queda esperando, apruébalo a mano y anótalo.

## 7. Modo enjambre

- [ ] Crea `C:\Users\<tu usuario>\Documents\EzAgent-pruebas\enjambre` y abre
      Herdr en un espacio `pruebas`.
- [ ] Pide: «Monta un enjambre en esa carpeta: OpenCode de planificador, Claude
      de desarrollador y Codex de QA. Que hagan un plan corto para una
      calculadora de sumas y lo implementen. Avísame al terminar.»
- [ ] Qué debes ver: el tablero arranca, cada miembro se abre **cuando le toca**
      (no todos a la vez), usan `ezs` para hablar y el supervisor avisa de los
      avances y del final.
- [ ] Entra en una terminal y comprueba que `ezs board` o `ezs next` responden y
      que el miembro tiene su asiento.
- [ ] Si falla: anota qué miembro no arrancó, si `ezs` no estaba en el `PATH`,
      el estado del tablero (`%APPDATA%\EzAgent\swarm\board.db`) y las líneas de
      `ezagent-swarm-sup` del log.

## 8. Modo autónomo A (una tarea)

Usa solo la carpeta de pruebas y una tarea trivial.

- [ ] Crea `...\EzAgent-pruebas\autonomo-a`.
- [ ] Di: «Hazlo en modo autónomo y autorizo los permisos. En la carpeta
      `...\autonomo-a`, crea `saludo.txt` con el texto «hola» y `nota.txt` con
      el texto «listo». Criterio: cada archivo contiene su texto; se comprueba
      con `type saludo.txt` y `type nota.txt`. Usa Codex en el espacio
      «pruebas».»
- [ ] Qué debes ver: una **única** confirmación generada por el código, que
      enumera los criterios y sus comprobaciones, la carpeta, el agente y los
      topes. Responde «sí».
- [ ] Observa que EzAgent abre el ejecutor, aprueba los permisos **una vez**
      (nunca «permitir siempre») y dice el inicio y las rondas.
- [ ] Al terminar, comprueba:
  - [ ] `saludo.txt` contiene `hola` y `nota.txt` contiene `listo`.
  - [ ] El aviso final nombra la tarea y la ruta de un resumen.
  - [ ] En `%APPDATA%\EzAgent\autonomo\<id>\privado\resumen.md` están los
        criterios con su evidencia y el diff.
  - [ ] En `compartido\` están `encargo.md` y `criterios.md`, y en
        `verificacion-1\` el `veredicto.json`.
- [ ] **Parada:** empieza otra tarea pequeña y di «para el modo autónomo» (o
      pulsa `Ctrl+Alt+E`, o haz clic en la fila «Autónomo · ronda N» de la vista
      rápida y confirma).
- [ ] Qué debes ver: «Detuve el modo autónomo.», no se aprueba nada más y la
      terminal del ejecutor sigue abierta.
- [ ] **Pausa:** si la tarea se pausa (topes, churn, diálogo desconocido), debe
      decir el motivo y ofrecer «sigue» o «para». Prueba «sigue» una vez.
- [ ] Si falla: anota el punto exacto (confirmación, aprobación, verificación,
      parada), el motivo de la pausa y las líneas de `ezagent-autonomo` del log.
      No incluyas capturas con claves.

## 9. Modo autónomo B (equipo)

- [ ] Crea `...\EzAgent-pruebas\autonomo-b`.
- [ ] Di: «Modo autónomo con equipo y autorizo los permisos: en
      `...\autonomo-b`, hagan una nota de planificación. `plan.md` debe tener
      tres pasos numerados y `resumen.md` debe repetir los tres pasos con la
      palabra «listo». Criterio: ambos archivos existen y `resumen.md` contiene
      «paso 1», «paso 2» y «paso 3»; se comprueba con `type plan.md` y
      `type resumen.md`. Papeles: OpenCode de planificador, Claude de
      desarrollador y Codex de QA.»
- [ ] Qué debes ver: la confirmación enumera papeles, agentes, carpeta y topes;
      los miembros se abren cuando les toca; la verificación la hace un
      verificador en una **sesión nueva** (por omisión, el mismo CLI del primer
      miembro) y no modifica archivos; si hay hallazgos, se crean tareas `fix`.
- [ ] Comprueba el resultado y el resumen final, igual que en el modo A.
- [ ] Si un miembro se queda en el diálogo de confianza de la carpeta,
      apruébalo tú, continúa y anótalo (en esta versión ese diálogo no siempre
      se aprueba solo).
- [ ] Si falla: anota el papel y el agente que falló, si la verificación se hizo
      en una sesión nueva y las líneas de `ezagent-autonomo` y
      `ezagent-swarm-sup`.

## 10. Menú de Ajustes

- [ ] Abre Ajustes con ☰ o `Ctrl+Alt+O`. Debe aparecer centrado y con cinco
      secciones: **Aspecto**, **Escucha**, **Voz**, **Modelos** y **Sistema**.
- [ ] **Aspecto:** cambia entre píldora y círculo, prueba los temas claro,
      oscuro y personalizado, y edita un color `#RRGGBB`. Mira que la barra y la
      ventana cambien sin reiniciar.
- [ ] **Escucha:** cambia de micrófono, mueve el umbral, apaga y enciende
      «Reactivar por voz» y cambia la frase.
- [ ] **Voz:** prueba «Prueba de voz», «Repetir lo último» y «Nueva
      conversación» (esta pide confirmación). Cambia la voz y la velocidad.
- [ ] **Modelos:** cambia de perfil y de modelo, pide «Modelos de la API» y
      guarda. Comprueba que el modelo elegido sigue ahí al reabrir.
- [ ] **Sistema:** edita un atajo, intenta poner dos iguales (debe rechazarlo),
      guarda y comprueba que se aplica sin reiniciar. Abre «Opciones avanzadas»
      y luego «Archivos»: deben abrir `system.md`, `permissions.toml`,
      `voice.json` y la carpeta de datos.
- [ ] Edita un campo de texto en línea, pega con `Ctrl+V`, cancela con `Esc` y
      confirma con `Enter`. Arrastra la ventana, redimensiónala y haz doble clic
      en la cabecera para volver al tamaño de fábrica.
- [ ] Guarda y comprueba el punto de «cambios sin guardar» y el check de
      confirmación.
- [ ] Si falla: anota la sección, el control y si el valor se perdió al
      reiniciar. Adjunta la sección de `voice.json` afectada **sin claves**.

## 11. Chat

- [ ] Abre el chat con 💬 o `Ctrl+Alt+T`.
- [ ] Qué debes ver: el panel nace de la forma de la barra con una animación
      corta y fluida; no se congela la barra ni el resto de la interfaz.
- [ ] Escribe «Hola, ¿qué puedes hacer?» y pulsa `Enter`.
- [ ] Qué debes ver: la respuesta aparece en streaming, con Markdown legible
      (listas, código o tabla si los hay), y se puede copiar.
- [ ] Pide una imagen: «Responde solo con esta línea:
      `![prueba](C:/Users/<tu usuario>/Pictures/prueba.png)`». Debe dibujarse en
      el chat y abrirse al pulsarla.
- [ ] Crea una conversación nueva, cambia a la anterior y bórrala (pide
      confirmación).
- [ ] Cierra el panel con su botón o el atajo. Vuelve a abrirlo: debe conservar
      el historial.
- [ ] Si falla: anota si el fallo es de animación, de teclado, de Markdown o de
      imagen, y guarda las líneas de `ezagent-chat-paint` y `ezagent-answer`.

## 12. Tema y monitor

- [ ] Con la barra abierta, cambia el tema y el movimiento reducido. Debe
      responder al instante.
- [ ] Si tienes un segundo monitor, arrastra la barra a él.
- [ ] Qué debes ver: la barra, la vista rápida, la tarjeta y Ajustes siguen el
      monitor sin deformarse, sin quedar cortados y con la nitidez correcta.
- [ ] Con la barra abierta, cambia la escala de Windows de 100 % a 150 % y
      vuelve a 100 % (Configuración → Pantalla → Escala).
- [ ] Qué debes ver: la barra se redibuja a la nueva escala, conserva una
      posición visible y los textos no se ven borrosos.
- [ ] Si falla: anota monitores, escalas, si la barra se salió de la pantalla o
      si hubo que reiniciar para recuperarla.

## 13. Suspensión y reanudación

- [ ] Con una llamada activa, suspende el equipo (Inicio → Suspender), espera
      15 s y vuelve a iniciarlo.
- [ ] Qué debes ver: la app sigue abierta, el micrófono se recupera y puedes
      hablar en unos segundos. Si Windows tardó, prueba a colgar y volver a
      llamar.
- [ ] Si falla: anota si la barra desapareció, si el micrófono quedó mudo o si
      hubo que reiniciar. Guarda el `engine.log` de esa transición.

## 14. Desconectar y reconectar el micrófono

- [ ] Durante una llamada, desconecta el micrófono USB.
- [ ] Qué debes ver: la app no se cierra ni se queda congelada; avisa o deja de
      recibir voz, y la barra sigue respondiendo.
- [ ] Vuelve a conectarlo. Elige el micrófono en Ajustes → Escucha si no se
      recupera solo y vuelve a llamar.
- [ ] Si falla: anota si se cayó la app, si el menú siguió abriendo y qué
      micrófono quedó seleccionado. Adjunta el `engine.log`.

## 15. Cortar la red un momento

- [ ] Haz una pregunta que necesite el LLM y, justo después, desactiva la red
      durante unos 15 s.
- [ ] Qué debes ver: una frase de error amable («No pude conectar con …»), sin
      que la barra se congele ni la app se cierre; el detalle queda en el log.
- [ ] Vuelve a activar la red y haz otra pregunta. Debe funcionar sin reiniciar.
- [ ] Si falla: anota el texto dicho, cuánto tardó en responder y las líneas del
      error en el `engine.log`.

## 16. Treinta minutos de uso normal

- [ ] Usa EzAgent como lo harías un día normal durante 30 minutos: llamadas,
      preguntas, alguna delegación, el chat y el menú.
- [ ] Abre el Administrador de tareas → Detalles → `ezagent-voice.exe` y anota
      la memoria (conjunto de trabajo privado) a los 5, 15 y 30 minutos.
- [ ] Qué debes ver: la memoria se mantiene en una banda estable; puede subir al
      principio (modelos y cachés) y oscilar, pero no crecer sin parar.
- [ ] Si falla: anota las tres cifras, qué estabas haciendo y si el crecimiento
      coincide con abrir el chat, el menú o las delegaciones. Guarda el
      `engine.log` completo.

## 17. Cierre

- [ ] Revisa que no haya errores repetidos en `engine.log` y que `audit.jsonl`
      no contenga claves.
- [ ] Rellena esta tabla y adjunta los registros de los puntos que fallaron.

| Punto | Resultado | Nota |
|---|---|---|
| 1. Arranque en el repo |  |  |
| 1. Instalador / otra cuenta |  |  |
| 2. Micrófono real |  |  |
| 3. Interrupción por voz |  |  |
| 4. «¿Te refieres a…?» |  |  |
| 5. LLM y herramientas |  |  |
| 6. Herdr (Codex, OpenCode, Claude) |  |  |
| 7. Modo enjambre |  |  |
| 8. Modo autónomo A |  |  |
| 9. Modo autónomo B |  |  |
| 10. Ajustes |  |  |
| 11. Chat e imágenes |  |  |
| 12. Tema y monitor |  |  |
| 13. Suspensión y reanudación |  |  |
| 14. Micrófono desconectado |  |  |
| 15. Red cortada |  |  |
| 16. Memoria en 30 min |  |  |

**Si algo falla, esto es lo que hay que enviar:** la hora, lo que dijiste o
hiciste, lo que esperabas y lo que pasó; las últimas 100 líneas de
`%APPDATA%\EzAgent\logs\engine.log`; y, si el fallo es de una herramienta,
la línea correspondiente de `audit.jsonl`. No envíes claves de API ni capturas
de la pantalla de Ajustes con la clave visible.
