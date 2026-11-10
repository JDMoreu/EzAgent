"""build_packet: qué parte de la salida de un agente pasa al siguiente."""

from __future__ import annotations

from ezagent.agent.handoff_packet import (
    DOCUMENT_FILE_CHARS,
    MAX_FALLBACK_CHARS,
    MIN_LOG_CHARS,
    SHORT_CHARS,
    build_packet,
    has_work,
    outline,
    packet_text,
)


def saver(store: list[str], path: str = "C:/tmp/registro.md"):
    def save(text: str) -> str:
        store.append(text)
        return path

    return save


# Un registro real: cada comando va precedido de su marca; la conclusión llega
# tras la última marca de herramienta.
WORK_LOG = (
    "\n".join(f"• Ran comando {i}\nsalida del comando {i}" for i in range(40))
    + "\n• Wrote C:/web/index.html\n"
    + "● Creé index.html en C:/web y quedó listo para publicar.\n"
)


def test_texto_vacio_da_un_paquete_corto_vacio() -> None:
    for empty in ("", "   ", None):
        packet = build_packet(empty)  # type: ignore[arg-type]
        assert (packet.kind, packet.text, packet.full) == ("short", "", "")


def test_una_respuesta_corta_sin_trabajo_se_pasa_tal_cual() -> None:
    packet = build_packet("Rojo\nAzul\nAmarillo")
    assert packet.kind == "short" and packet.text == "Rojo\nAzul\nAmarillo"
    assert packet.saved_path == ""


def test_el_limite_de_corto_es_inclusivo() -> None:
    assert build_packet("a" * SHORT_CHARS).kind == "short"
    assert build_packet("a" * (SHORT_CHARS + 1)).kind == "document"


def test_un_texto_largo_sin_trabajo_se_pasa_entero_hasta_el_limite_de_archivo() -> None:
    plan = "palabra " * ((DOCUMENT_FILE_CHARS - 10) // 8)
    store: list[str] = []
    packet = build_packet(plan, save=saver(store))
    assert packet.kind == "document" and packet.text == plan.strip()
    assert store == [] and packet.saved_path == ""


def test_un_documento_muy_largo_se_guarda_y_se_pasa_un_indice() -> None:
    plan = "# Plan\n" + "\n".join(f"{i}. Paso número {i} del plan" for i in range(1, 400))
    assert len(plan) > DOCUMENT_FILE_CHARS
    store: list[str] = []
    packet = build_packet(plan, save=saver(store))
    assert packet.kind == "document" and store == [plan]
    assert packet.saved_path == "C:/tmp/registro.md"
    assert len(packet.text) < len(plan) / 2
    assert "Contenido:" in packet.text
    assert packet.full == plan


def test_un_documento_muy_largo_sin_guardador_se_pasa_entero() -> None:
    plan = "x " * DOCUMENT_FILE_CHARS
    assert build_packet(plan).text == plan.strip()


def test_hay_trabajo_cuando_aparecen_marcas_de_herramientas() -> None:
    for line in [
        "Ran 1 shell command",
        "• Ran uv sync",
        "● Wrote a.txt",
        "# Wrote /tmp/a",
        "→ Read x",
        "Explored: src",
    ]:
        assert has_work(line), line
    assert not has_work("Hice un plan con varios pasos.\n1. uno\n2. dos")
    assert not has_work("")


def test_con_trabajo_solo_pasa_la_conclusion() -> None:
    store: list[str] = []
    packet = build_packet(WORK_LOG, save=saver(store))
    assert packet.kind == "report"
    assert packet.text == "Creé index.html en C:/web y quedó listo para publicar."
    assert "salida del comando" not in packet.text
    assert packet.full == WORK_LOG.strip()


def test_el_registro_largo_se_guarda_y_el_corto_no() -> None:
    long_store: list[str] = []
    assert build_packet(WORK_LOG, save=saver(long_store)).saved_path
    assert long_store == [WORK_LOG.strip()]

    short_store: list[str] = []
    brief = "Ran 1 shell command\n● Listo.\n"
    assert len(brief) - len("Listo.") < MIN_LOG_CHARS
    assert build_packet(brief, save=saver(short_store)).saved_path == ""
    assert short_store == []


def test_si_guardar_falla_no_se_inventa_una_ruta() -> None:
    packet = build_packet(WORK_LOG, save=lambda text: None)
    assert packet.kind == "report" and packet.saved_path == ""


def test_el_selector_del_modelo_solo_mueve_la_linea_de_inicio() -> None:
    lines = [f"linea {i}" for i in range(1, 31)]
    text = "Ran 1 shell command\n" + "\n".join(lines)
    packet = build_packet(text, select=lambda ls: 25)
    assert packet.text.splitlines()[0] == ls_at(text, 25)
    # El modelo no puede introducir texto: lo que se pasa es un fragmento literal.
    assert packet.text in text


def ls_at(text: str, number: int) -> str:
    return text.splitlines()[number - 1]


def test_un_selector_hostil_o_roto_se_ignora() -> None:
    text = "\n".join(f"• Ran c{i}\nl{i}" for i in range(15)) + "\n• Wrote x\n● Fin\n"

    def boom(lines):
        raise RuntimeError("modelo caído")

    for bad in (
        boom,
        lambda ls: 0,
        lambda ls: -3,
        lambda ls: 10_000,
        lambda ls: "inyección",
        lambda ls: None,
        lambda ls: 2.5,
    ):
        packet = build_packet(text, select=bad)
        assert packet.text == "Fin", bad


def test_sin_modelo_se_limita_la_conclusion_por_lineas_enteras() -> None:
    tail = "\n".join(f"resultado {i:04d}" for i in range(400))
    packet = build_packet("Ran 1 shell command\n" + tail)
    assert len(packet.text) <= MAX_FALLBACK_CHARS
    assert packet.text.splitlines()[-1] == "resultado 0399"
    assert all(
        line.startswith("resultado ") and len(line) == 14 for line in packet.text.splitlines()
    )


def test_packet_text_agrega_la_nota_con_la_ruta() -> None:
    store: list[str] = []
    report = build_packet(WORK_LOG, save=saver(store))
    assert "Registro completo del trabajo" in packet_text(report)
    assert "C:/tmp/registro.md" in packet_text(report)
    assert packet_text(report).startswith(report.text)

    document = build_packet(
        "# Plan\n" + "\n".join(f"{i}. Paso {i}" for i in range(1, 900)), save=saver(store)
    )
    assert "Texto completo" in packet_text(document)

    plain = build_packet("hola")
    assert packet_text(plain) == "hola"


def test_outline_toma_primer_parrafo_y_titulos_sin_duplicados() -> None:
    text = "Plan de la landing\n\nObjetivo\nTexto largo que termina en punto.\nObjetivo\nPublicar\n"
    out = outline(text)
    assert out.startswith("Plan de la landing")
    assert out.count("- Objetivo") == 1 and "- Publicar" in out


def test_outline_vacio() -> None:
    assert outline("") == ""
