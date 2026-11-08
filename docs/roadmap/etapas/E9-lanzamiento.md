# E9 · Lanzamiento open source

| Campo | Valor |
|---|---|
| Depende de | E8 |
| Agente sugerido | DeepSeek V4.1 Flash; landing con Claude Code + skill `design-taste-frontend` |
| Esfuerzo | S (1–2 días) |
| Rama | `stage/E9-lanzamiento` |
| Hallazgos | D5, P4 (cierre) |

## Objetivo
Publicar la versión 0.2.0 con un README que se entienda en 30 segundos, una
demo real, releases reproducibles y un proyecto preparado para recibir
contribuciones.

## Dentro del alcance
- T1 Decisión de nombre (`01-producto-y-marca.md` §5): comprobación de
  dominio, PyPI, GitHub y marcas. Si cambia, renombrado en un solo commit.
- T2 README bilingüe (`README.md` en inglés + `README.es.md`): una frase,
  GIF de demo de 20–30 s (lanzar un agente por voz y recibir el aviso),
  diagrama, requisitos, instalación, proveedores, privacidad (qué va a la
  nube: solo texto al LLM), comparativa honesta de alcance y limitaciones.
- T3 Landing / docs públicas (GitHub Pages) con la skill
  `design-taste-frontend`, usando los tokens de `design/tokens.json`.
- T4 `release.yml` final: tag `v*` → pruebas → build → instalador + zip +
  `SHA256SUMS` → GitHub Release con notas desde `CHANGELOG.md`.
- T5 Comunidad: `CHANGELOG.md` (Keep a Changelog), etiquetas
  (`good first issue`, `help wanted`, `stage:*`), 5–10 issues iniciales
  sacadas de las «pendientes» de cada etapa, Discussions activadas.

## Fuera del alcance
- Telemetría (no se añade; si algún día se añade, será opt-in y documentada).
- Publicación en PyPI (opcional, después).

## Criterios de aceptación
- [ ] Una persona ajena al proyecto entiende qué hace leyendo solo la primera
      pantalla del README (prueba con 2 personas; anotar resultado).
- [ ] El GIF muestra un uso real, sin cortes engañosos.
- [ ] Ninguna cifra del README sale de fuera de `docs/roadmap/metricas.md`.
- [ ] Release `v0.2.0` publicada con artefactos y hashes.
- [ ] Landing con contraste AA y sin errores de consola.

## Prompt para el agente
```
Lee AGENTS.md, docs/roadmap/etapas/E9-lanzamiento.md,
docs/roadmap/01-producto-y-marca.md y docs/roadmap/metricas.md.
Ejecuta la tarea <T#> de E9. No inventes cifras, testimonios ni
compatibilidades: todo debe salir de metricas.md o del código. Entrega el
informe de AGENTS.md §6.
```
