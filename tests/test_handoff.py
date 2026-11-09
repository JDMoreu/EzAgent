"""Traspaso entre agentes: solo la respuesta, sin pantalla ni explicaciones."""

from __future__ import annotations

from pathlib import Path

import pytest

from ezagent.agent.handoff import (
    answer_or_tail,
    clean_tail,
    compose_handoff,
    extract_answer,
)

FIXTURES = Path(__file__).parent / "fixtures" / "screens"


@pytest.mark.parametrize(
    ("agent", "expected"),
    [
        ("opencode", "Azul\nAmarillo\nRojo"),
        ("claude", "Rojo\nAzul\nAmarillo"),
        ("mcode", "Rojo\nAmarillo\nAzul"),
        ("codex", "Rojo\nAmarillo\nAzul"),
    ],
)
def test_extrae_solo_la_respuesta_de_capturas_reales(agent, expected):
    screen = (FIXTURES / f"{agent}.txt").read_text(encoding="utf-8")
    assert extract_answer(screen) == expected


def test_pantalla_de_codex_sin_eco_hc_usa_la_tarea_enviada():
    # El caso real del log: la pantalla completa de Codex acabó en OpenCode.
    screen = (
        ">_ OpenAI Codex (v0.167.0)\n"
        "~\\Documents\\EzAgent\n"
        "› Dame una sola pregunta de cultura general, corta y clara. "
        "Responde únicamente con la pregunta, sin la respuesta ni\n"
        "comentarios.\n"
        "• ¿Cuál es el planeta más grande del sistema solar?\n"
        "Worked for 2s • 1:12 PM\n"
        "› Ask Codex to do anything\n"
        "GPT-6-Luna default · ~\\Documents\\EzAgent\n"
        "← for agents · ? for shortcuts\n"
    )
    task = (
        "Dame una sola pregunta de cultura general, corta y clara. "
        "Responde únicamente con la pregunta, sin la respuesta ni comentarios."
    )
    expected = "¿Cuál es el planeta más grande del sistema solar?"
    assert extract_answer(screen, task) == expected
    # Y sin conocer la tarea, la viñeta de respuesta basta.
    assert extract_answer(screen) == expected


def test_salta_el_razonamiento_y_conserva_varios_parrafos():
    screen = (
        "› <hc from=\"t1\">explica</hc>\n"
        "└ • Thought for 2.0s\n"
        "● Primer párrafo.\n\n"
        "Segundo párrafo.\n"
        "└ Completed in 2s · ⚡ 857 tok/s\n"
    )
    assert extract_answer(screen) == "Primer párrafo.\n\nSegundo párrafo."


def test_una_viñeta_de_markdown_con_for_no_corta_la_respuesta():
    screen = '❯ <hc from="t1">lista</hc>\n● Pasos:\n* Hornear for 3 minutos\n'
    assert extract_answer(screen).endswith("* Hornear for 3 minutos")


def test_sin_respuesta_reconocible_devuelve_vacio_y_el_respaldo_limpia():
    screen = "ruido\n╹▀▀▀▀▀▀▀▀▀\nBuild · Modelo · 2.4s\nmás ruido\n"
    assert extract_answer(screen) == ""
    assert "Build" not in clean_tail(screen)
    assert answer_or_tail(screen) == clean_tail(screen)


def test_componer_con_marcador_output_pega_la_respuesta_en_su_sitio():
    assert (
        compose_handoff("Responde esto brevemente: {output}", "¿Qué es X?")
        == "Responde esto brevemente: ¿Qué es X?"
    )


def test_componer_sin_marcador_anade_la_respuesta_delimitada():
    text = compose_handoff("Audita el plan", "1. a\n2. b")
    assert text == 'Audita el plan\n\n"""\n1. a\n2. b\n"""'
    # Nada de explicaciones sobre terminales ni sobre "instrucciones".
    assert "t2t" not in text
    assert "salida del agente" not in text


def test_componer_sin_respuesta_deja_solo_la_tarea():
    assert compose_handoff("Haz X", "") == "Haz X"


def test_opencode_con_razonamiento_largo_devuelve_solo_la_respuesta():
    # Pantalla real de OpenCode (deepseek-v4.1-flash): «+ Thought · 1.5s».
    screen = (FIXTURES / "opencode_long.txt").read_text(encoding="utf-8")
    answer = extract_answer(screen)
    assert answer.startswith("El océano Pacífico.")
    assert answer.endswith("sin verificación externa porque es un dato estable.")
    assert "Thought" not in answer
    assert "Build" not in answer


def test_ancla_en_el_texto_pegado_aunque_la_pantalla_lo_parta():
    # Pantalla real de OpenCode: sin etiqueta <hc>, con el plan y el contexto
    # repartidos en varias líneas y razonamiento «+ Thought».
    screen = (FIXTURES / "opencode_context.txt").read_text(encoding="utf-8")
    pasted = (
        "Este es el plan que se genero con Claude para una landing page sencilla:\n"
        "1. Define y escribe el contenido: objetivo único, titular, 3 beneficios, y un botón de acción (CTA).\n"
        "2. Maqueta en un solo index.html con HTML semántico y CSS simple (hero, beneficios, CTA, pie), responsive con flexbox/grid.\n"
        "3. Pule lo esencial: tipografía legible, buen contraste, imágenes optimizadas y revisión en móvil.\n"
        "4. Publica gratis en GitHub Pages, Netlify o Vercel, y comprueba que el enlace y el CTA funcionan.\n\n"
        "Reglas: no modifiques ni crees archivos todavia. Solo resume este plan en UNA frase y dime cual es el primer paso."
    )
    answer = extract_answer(screen, pasted)
    assert answer.startswith("Resumen: Crear una landing sencilla")
    assert answer.endswith("no una tarea técnica.")
    assert "Este es el plan" not in answer
    assert "Reglas:" not in answer
    assert "Build" not in answer


def test_un_plan_largo_pasa_entero_sin_recorte_de_6000_caracteres():
    body = "\n".join(f"{i}. Paso detallado número {i} con su explicación." for i in range(400))
    screen = f'❯ <hc from="t1">haz el plan</hc>\n● {body}\nWorked for 40s\n'
    answer = extract_answer(screen)
    assert len(answer) > 15_000
    assert answer.endswith("Paso detallado número 399 con su explicación.")
