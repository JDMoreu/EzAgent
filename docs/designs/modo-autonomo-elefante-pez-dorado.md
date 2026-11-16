# Modo autónomo y bucle multiagente (elefante y pez dorado)

Estado: **borrador para revisión y aprobación**. No se implementa nada hasta que el
usuario lo apruebe.

## 1. Pedido

1. **Modo autónomo (YOLO).** «Cuando yo te diga explícitamente que complete la tarea
   de forma autónoma y que autorizo los permisos, no me preguntes cada vez que un
   agente pida permiso de ejecución: ya te di mi autorización anticipada. En ese modo
   debe entrar en un bucle que ejecute la tarea que pedí y solo se pause o termine
   cuando se cumpla la tarea asignada.»
2. **Bucle autónomo multiagente.** «Yo declaro la tarea, los agentes (planificador,
   desarrollador, auditor, tester, lo que sea) y el resultado que necesito, y tú te
   encargas de validar que el resultado cumpla los requisitos que especifiqué. El
   elefante mantiene el contexto y no ejecuta la tarea; los peces dorados ejecutan;
   tú serías el elefante.»

## 2. La metodología (Elephant-Goldfish Model, Google)

Fuentes: el ensayo de Dave Rensin (Google), el README de `google/elephant-goldfish-model`
y la descripción del patrón en agentpatterns.ai (lista al final).

- **Elefante:** sesión larga que guarda el contexto del proyecto (objetivos, decisiones,
  restricciones) y redacta artefactos escritos. **No escribe código de producción**
  y **no califica su propio trabajo**: «la sesión que escribió el borrador está de
  acuerdo con él».
- **Pez dorado:** sesión **nueva, sin memoria** de la conversación. Recibe solo el
  documento, los archivos que este cita y su papel. Si no entiende el documento, el
  fallo es del documento. Se crea uno nuevo en cada ronda y **nunca se reanuda ni se
  bifurca**: «un pez que recibe la transcripción del elefante ya no es un pez».
- **Persona:** decide las compensaciones y aprueba. «La IA es una herramienta, no
  quien decide.»
- **Hallazgos:** los peces devuelven hallazgos clasificados en Bloqueante, Mayor y
  Menor. El elefante responde a cada uno con un veredicto: corregir, discutir con
  evidencia que el pez pueda leer, escalar a la persona o descartar. Los Menores no
  bloquean.
- **Parada:** **dos rondas limpias seguidas**, porque «el siguiente pez nuevo puede
  encontrar otro hueco en el mismo texto». Con un **tope de cinco rondas como punto
  de control**, no como límite duro: ahí la persona decide si se acepta, se sigue o
  se vuelve a empezar. Si un hallazgo nuevo cae justo en lo que arregló la ronda
  anterior («churn»), se escala. Una ronda limpia significa ningún Bloqueante ni
  Mayor sin resolver.
- **Regla de cero decisiones:** un plan no puede dejar al ejecutor elegir un esquema,
  una API o una dependencia.

### Adaptación a EzAgent

En el modelo de Google los peces dorados **revisan** diseños. Aquí se generaliza la
regla de aislamiento para que los peces también **ejecuten**:

| Papel | En EzAgent |
|---|---|
| Elefante | EzAgent (el mensajero de voz). Guarda la tarea, los criterios, el estado y el historial; escribe los encargos; decide; valida. **Nunca ejecuta la tarea**: no edita archivos del proyecto ni corre sus comandos. |
| Peces ejecutores | Agentes de código (OpenCode, Claude Code, Codex…) en terminales de Herdr. Cada uno recibe **solo su encargo escrito** y las rutas que necesita. |
| Peces verificadores | Agentes en **sesiones nuevas en cada ronda**. Reciben solo la tarea, los criterios y dónde mirar. Nunca reciben la conversación ni los hallazgos de rondas anteriores. |
| Persona | Aprueba la tarea, los criterios y la activación del modo. Responde a las escaladas. |

## 3. Modo A: tarea autónoma con autorización anticipada

### Activación
1. El usuario lo pide de forma explícita: por ejemplo, «hazlo en modo autónomo,
   autorizo los permisos».
2. El modelo llama a la herramienta nueva `start_autonomous_task` con `task`,
   `done_criteria`, `agent`, `space` y, opcionalmente, `terminal`.
3. Si `done_criteria` está vacío o no es verificable, el elefante pregunta antes de
   empezar.
4. **Una única confirmación generada por el código**, nunca por el modelo:
   «Modo autónomo: haré «{tarea}» con {agente} en el espacio «{espacio}» y aprobaré
   los permisos que pida el agente hasta que se cumpla «{criterio}», como máximo
   {límite}. ¿Lo activo?». Se responde «sí» o «no», por voz o con botón.
5. **Solo el usuario puede activarlo.** La herramienta se rechaza si el turno está
   marcado como contaminado (`taint`: ya se leyó web, archivos o salida de agentes en
   ese turno) o si la petición no vino de un mensaje del usuario. Ningún texto de un
   agente, de la web o de un archivo puede encender el modo.

### Qué cubre la autorización
- **Solo** los diálogos de permiso de las terminales lanzadas para esta tarea. No
  cubre otras terminales ni otros espacios.
- **No** cubre las herramientas propias de EzAgent (borrar o mover archivos del
  usuario, etc.). Esas siguen pidiendo confirmación, porque el elefante no ejecuta.
- **Aprobación de una sola vez.** En el diálogo se elige siempre la opción «permitir
  esta vez» (o equivalente), **nunca** «permitir siempre» o «no volver a preguntar».
- **Lista de denegación.** Si el comando o la acción que se pide coincide con algo
  catastrófico (borrar la raíz, el perfil del usuario o una unidad; formatear;
  `git push --force` sobre ramas principales; ejecutar scripts descargados con
  `| sh` o `| iex`; tocar credenciales; apagar el equipo), **no se aprueba**: el
  bucle se pausa y se pregunta al usuario. La lista vive en el código, no en el modelo.
- **Auditoría.** Cada aprobación automática queda en la auditoría
  (`autonomous.approve`), con la terminal, la opción elegida y un extracto saneado
  de la pantalla. Al terminar, el resumen dice cuántas hubo.

### Cómo se aprueba un permiso
1. El vigía ve `blocked` en una terminal de la tarea.
2. Se lee la pantalla y se clasifica el diálogo:
   - **Permiso de ejecución o de edición:** se aprueba según lo anterior.
   - **Confianza en una carpeta** (el `_BLOCKING_DIALOG` actual): solo se aprueba si
     la carpeta es la del espacio o proyecto de la tarea.
   - **Cualquier otro:** se pausa y se pregunta.
3. La tecla de aprobación se deduce del diálogo: las opciones numeradas, y la que dice
   «sí», «yes», «allow» o «permitir» **para esta vez**. Para los CLIs conocidos queda
   fijada en el perfil del agente (`approve_keys`). Si no se puede deducir con
   seguridad, se pregunta.

### Bucle
`preparando → ejecutando → verificando → { ejecutando (corrección) | cumplida | en pausa | detenida }`

- **Ejecutando:** el pez ejecutor trabaja; los permisos se aprueban solos.
- **Verificando:** cuando el ejecutor termina (`done` o en reposo), el elefante lanza
  un **pez verificador nuevo**, con un perfil de verificador y la instrucción de no
  modificar nada. Recibe solo `encargo.md`, `criterios.md` y las rutas, y responde con
  un veredicto estructurado: `CUMPLE` o `NO CUMPLE`, con hallazgos (Bloqueante, Mayor
  o Menor) y evidencia.
- **Corrección:** si no cumple, el elefante escribe `correccion-N.md` (solo los
  hallazgos aceptados, sin la conversación) y se lo manda al ejecutor (`send`); vuelve
  a «ejecutando».
- **Cumplida:** **dos verificaciones limpias seguidas, cada una con un pez nuevo**. El
  elefante lo anuncia: «Terminé «{tarea}»: se cumple «{criterio}». Aprobé {n}
  permisos.»
- **En pausa (pregunta al usuario):** lo que ocurra primero entre:
  - tope de rondas (5 por defecto) o de tiempo (2 h por defecto);
  - churn;
  - algo en la lista de denegación;
  - un diálogo desconocido;
  - un incidente de cuota, contexto lleno o ventana cerrada (se reutiliza la
    recuperación del enjambre);
  - el mismo error dos veces seguidas.
- **Detenida:** el usuario dice «para el modo autónomo», usa el atajo de cancelar o
  pulsa el botón de la barra. Se retira la autorización al instante.

### Interfaz
- **Barra:** marca visible del modo autónomo mientras dura, con el estado y la ronda
  en la vista rápida de actividad.
- **Voz:** solo avisos de cambio de estado: inicio, ronda N, pausa con su motivo y
  fin. No narra cada permiso.

## 4. Modo B: bucle multiagente

### Declaración
El usuario dice la tarea, los papeles (planificador, desarrollador, auditor, tester,
etc.), con qué agente y modelo va cada uno, y el resultado que necesita.

- El elefante redacta `criterios.md` con criterios **verificables**: qué comando,
  prueba o comprobación demuestra cada uno.
- Si algo queda ambiguo, pregunta antes de empezar (regla de cero decisiones).
- Lee el resumen al usuario y pide **una sola confirmación**, la del modo A, que
  cubre a todo el equipo.

### Sobre el modo enjambre existente
Se apoya en lo que ya hay en `ez-swarm`: tablero de tareas, papeles, supervisor que
lanza y despierta, incidentes, recuperación y avisos de avance.

- Los miembros son peces dorados. Cada uno recibe por `ezs` **solo su tarea escrita**
  y las rutas.
- El elefante no ejecuta. Crea tareas en el tablero, lee resultados y decide.

### Una ronda
1. **Plan:** el planificador escribe `plan.md` a partir de `encargo.md` y
   `criterios.md`.
2. **Revisión del plan:** un pez nuevo revisa la comprensión y la regla de cero
   decisiones. Si hay Bloqueantes o Mayores, el plan se corrige.
3. **Ejecución:** desarrollador y demás papeles, en paralelo donde se pueda.
4. **Verificación:** auditor y tester **nuevos en cada ronda**, contra `criterios.md`.
   Escriben `ronda-N/hallazgos.md`.
5. **Resolución:** el elefante da un veredicto a cada hallazgo (corregir → tarea nueva
   al papel que toca; discutir con evidencia; escalar; descartar los Menores). Luego
   viene la ronda siguiente.

Las reglas de parada son las del modo A: dos rondas limpias seguidas para terminar;
tope de rondas o tiempo, churn o incidente para pausar.

### Artefactos
Carpeta `%APPDATA%/EzAgent/autonomo/<id>/`:
- `encargo.md`, `criterios.md` y `plan.md`;
- `ronda-N/hallazgos.md`;
- `estado.json` (estado, ronda, contadores y terminales cubiertas);
- `bitacora.jsonl`.

Sirven para retomar tras un reinicio, como hace hoy `tasks.json`.

## 5. Seguridad (no negociable)

- **Activación:** solo por petición explícita del usuario, con una confirmación
  generada por el código. Nunca desde datos no confiables (web, archivos, salida de
  agentes).
- **Alcance:** solo las terminales de la tarea; aprobación de una sola vez; lista de
  denegación en el código; auditoría de cada aprobación.
- **Límites:** de rondas y de tiempo, y parada inmediata por voz, atajo o botón.
- **El elefante no ejecuta:** sus herramientas propias de riesgo siguen pidiendo
  confirmación.
- **Sin flags de los CLIs:** ningún permiso se concede con opciones como
  `--dangerously-skip-permissions`. Saltarían la lista de denegación y la auditoría.

## 6. Plan de implementación (tras la aprobación)

| Fase | Contenido | Crates |
|---|---|---|
| A1 | Estado y artefactos del modo, herramienta `start_autonomous_task`, confirmación única, parada | `ez-agent`, `ez-app` |
| A2 | Clasificador de diálogos de permiso, lista de denegación, tecla de aprobación, auditoría | `ez-herdr`, `ez-agent` |
| A3 | Pez verificador en sesión nueva, veredicto estructurado, corrección, reglas de parada | `ez-herdr`, `ez-agent`, `ez-app` |
| B1 | Declaración multiagente y `criterios.md` sobre el enjambre | `ez-swarm`, `ez-agent` |
| B2 | Rondas con plan, revisión, ejecución y verificación; veredictos del elefante | `ez-swarm`, `ez-app` |
| UI | Marca en la barra, vista rápida y avisos de voz | `ez-hud`, `ez-app` |

Cada fase lleva sus pruebas con dobles de Herdr y de los agentes (`fake-hc`,
`fake-agent`). Además, un escenario de reproducción nuevo cubre el bucle de punta a
punta.

## 7. Alternativas descartadas

- **Aprobar todo sin alcance** (cualquier terminal, opción «siempre»): se descarta por
  seguridad.
- **Que el elefante ejecute o corrija él mismo:** contradice el modelo y contamina su
  contexto.
- **Verificar con el mismo agente que ejecutó, o reanudar a un verificador:** introduce
  sesgo, porque quien hizo el trabajo está de acuerdo con él.
- **Activar los flags de «saltar permisos» de cada CLI:** no todos los tienen, y
  saltan la lista de denegación y la auditoría.
- **Una ronda limpia para dar por cumplido:** el modelo de Google demuestra que no
  basta.

## 8. Decisiones del usuario (con recomendación)

1. **¿La autorización cubre también las herramientas propias de EzAgent?**
   Recomendado: **no**; solo los permisos de los agentes.
2. **¿La lista de denegación sigue activa en modo autónomo?**
   Recomendado: **sí**; pausa y pregunta.
3. **Límites por defecto.** Recomendado: **5 rondas y 2 horas**, configurables; al
   llegar al tope se pausa y se pregunta.
4. **Verificador.** Recomendado: el mismo CLI que el ejecutor pero en sesión nueva, o
   el agente que el usuario nombre como auditor o tester.

## Fuentes

- Dave Rensin, «Elephants, Goldfish and the New Golden Age of Software Engineering»,
  Google Research: https://research.google/pubs/elephants-goldfish-and-the-new-golden-age-of-software-engineering/
- Repositorio: https://github.com/google/elephant-goldfish-model
- Descripción del patrón: https://agentpatterns.ai/patterns/multi-agent/elephant-goldfish-design-review/
