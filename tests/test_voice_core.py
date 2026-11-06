"""Tests del núcleo de ezagent-voice (sin micrófono ni Herdr real)."""

from __future__ import annotations

import wave
from io import BytesIO

import pytest

from ezagent.voice.audio import Segmenter, pcm_to_wav
from ezagent.voice.config import VoiceConfig
from ezagent.voice.herdr import Herdr, strip_ansi
from ezagent.voice.hotkeys import parse_hotkey
from ezagent.voice.llm import Messenger
from ezagent.voice.watcher import Watcher


def test_pcm_to_wav_uses_native_rate():
    wav = pcm_to_wav(b"\x00\x00" * 441, 44100)
    with wave.open(BytesIO(wav), "rb") as handle:
        assert handle.getframerate() == 44100
        assert handle.getnchannels() == 1
        assert handle.getnframes() == 441


def test_segmenter_does_not_cut_on_short_pauses():
    segmenter = Segmenter(
        min_rms=0.003,
        silence_secs=1.8,
        pre_roll_secs=0.5,
        min_speech_secs=0.3,
        noise_multiplier=1.8,
    )
    chunk = b"\x00\x00" * 480  # 30 ms a 16 kHz
    started = False
    utterance = None

    for _ in range(10):  # calibrar el ruido
        _, start = segmenter.feed(chunk, 0.002)
        started |= start
    for _ in range(20):  # 0,6 s de voz
        pcm, start = segmenter.feed(chunk, 0.02)
        started |= start
        utterance = pcm or utterance
    assert started

    for _ in range(16):  # pausa de 0,48 s: no debe cerrar el turno
        pcm, _ = segmenter.feed(chunk, 0.002)
        assert pcm is None

    for _ in range(10):  # reanudar la voz
        pcm, _ = segmenter.feed(chunk, 0.02)
        assert pcm is None

    for _ in range(61):  # 1,83 s de silencio: ahora sí
        pcm, _ = segmenter.feed(chunk, 0.002)
        if pcm:
            utterance = pcm
            break

    assert utterance is not None
    assert len(utterance) > 0


def test_segmenter_adapts_to_noise_and_waits_25s():
    segmenter = Segmenter()  # defaults: min_rms 0.0006, silencio 2.5 s
    chunk = b"\x00\x00" * 480
    for _ in range(20):  # ruido de fondo
        segmenter.feed(chunk, 0.004)
    started = False
    for _ in range(20):  # 0,6 s de voz
        _, start = segmenter.feed(chunk, 0.02)
        started |= start
    assert started

    for _ in range(50):  # pausa de 1,5 s: no debe cerrar
        pcm, _ = segmenter.feed(chunk, 0.004)
        assert pcm is None

    for _ in range(10):  # reanudar
        segmenter.feed(chunk, 0.02)

    pcm = None
    for _ in range(85):  # 2,55 s de silencio: ahora sí
        pcm, _ = segmenter.feed(chunk, 0.004)
        if pcm:
            break
    assert pcm is not None


def test_segmenter_detecta_una_frase_que_empieza_al_arrancar():
    """La calibración inicial no debe confundir una frase variable con ruido."""
    segmenter = Segmenter(
        min_rms=0.0006,
        silence_secs=0.5,
        pre_roll_secs=0.5,
        min_speech_secs=0.2,
        noise_multiplier=1.3,
    )
    chunk = b"\x00\x00" * 480
    levels = (0.004, 0.006, 0.003, 0.005, 0.007, 0.004, 0.006, 0.005, 0.004, 0.006)
    started = False
    for level in levels:
        _, start = segmenter.feed(chunk, level)
        started |= start
    assert started


def test_segmenter_detecta_voz_floja_sin_aprenderla_como_ruido():
    """Una frase por debajo del umbral no debe subir el suelo del VAD."""
    segmenter = Segmenter(
        min_rms=0.0006,
        silence_secs=0.5,
        pre_roll_secs=0.5,
        min_speech_secs=0.2,
        noise_multiplier=1.3,
    )
    chunk = b"\x00\x00" * 480
    for _ in range(25):  # calibración en una habitación muy silenciosa
        segmenter.feed(chunk, 0.00003)
    for _ in range(40):  # voz floja que aún no cruza el umbral
        segmenter.feed(chunk, 0.0005)
    assert segmenter.noise < 0.0001

    started = False
    for _ in range(10):  # la misma voz, ahora por encima del umbral
        _, start = segmenter.feed(chunk, 0.001)
        started |= start
    assert started


def test_config_migra_los_defaults_antiguos_del_vad(tmp_path):
    import json

    path = tmp_path / "voice.json"
    path.write_text(
        json.dumps({"min_rms": 0.0015, "min_speech_secs": 0.3}),
        encoding="utf-8",
    )
    cfg = VoiceConfig.load(path)
    assert cfg.min_rms == 0.0006
    assert cfg.min_speech_secs == 0.2
    assert cfg.vad_defaults_version == 2


def test_la_cola_del_tts_no_descarta_una_frase_que_continua():
    """Si la frase empieza con el eco pero sigue viva, no se tira."""
    import time

    from ezagent.voice.app import VoiceApp

    app = VoiceApp.__new__(VoiceApp)
    app.in_call = True
    app.muted = False
    app._ignore_utterance = False
    app._post_tts_ignore = False
    app._post_tts_until = time.monotonic() + 60
    app._user_speaking = False
    app.state = "listening"

    app._on_speech_start(0.02)
    assert app._ignore_utterance is True
    assert app._post_tts_ignore is True

    app._post_tts_until = time.monotonic() - 1
    app._on_utterance(b"", 16000)
    assert app._ignore_utterance is False
    assert app._post_tts_ignore is False


def test_latency_report_y_percentiles():
    from ezagent.voice.metrics import LatencyStats, TurnTimings

    timings = TurnTimings(speech_end=10.0, vad_end=10.4)
    timings.marks.update(
        {
            "stt_ready": 10.7,
            "first_token": 11.0,
            "first_sentence": 11.2,
            "first_sound": 11.3,
            "turn_end": 13.0,
        }
    )
    report = timings.report()
    assert report["vad_wait_ms"] == pytest.approx(400)
    assert report["stt_ready_ms"] == pytest.approx(300)
    assert report["first_token_ms"] == pytest.approx(300)
    assert report["first_sentence_ms"] == pytest.approx(200)
    assert report["first_sound_ms"] == pytest.approx(100)
    assert report["ttfa_ms"] == pytest.approx(1300)
    assert report["total_ms"] == pytest.approx(1300)

    assert LatencyStats.percentile([10, 20, 30], 50) == pytest.approx(20)
    assert LatencyStats.percentile([10, 20, 30], 95) == pytest.approx(29)
    assert LatencyStats.percentile([], 95) == 0.0


def test_latency_summary_no_se_repite_tras_la_ventana(capsys):
    from ezagent.voice.metrics import LatencyStats, TurnTimings

    stats = LatencyStats(maxlen=10)
    for index in range(12):
        timing = TurnTimings(speech_end=index, vad_end=index + 0.2)
        timing.marks["stt_ready"] = index + 0.4
        timing.marks["first_token"] = index + 0.6
        timing.marks["first_sound"] = index + 0.8
        timing.marks["turn_end"] = index + 1.0
        stats.record(timing)
    output = capsys.readouterr().out
    assert output.count("Latencia acumulada") == 1


def test_segmenter_marca_el_fin_de_voz():
    segmenter = Segmenter(min_rms=0.0006, silence_secs=0.3)
    chunk = b"\x00\x00" * 480
    for _ in range(20):
        segmenter.feed(chunk, 0.00003)
    started = False
    for _ in range(10):
        _, start = segmenter.feed(chunk, 0.02)
        started |= start
    assert started
    utterance = None
    for _ in range(20):
        pcm, _ = segmenter.feed(chunk, 0.00003)
        if pcm:
            utterance = pcm
            break
    assert utterance is not None
    assert segmenter.last_speech_end is not None


def test_sentences_emite_una_primera_clausula_corta():
    from ezagent.voice.app import _Sentences

    splitter = _Sentences()
    chunks = splitter.feed("Voy a revisar el estado, y te digo qué encuentro.")
    assert chunks
    assert chunks[0] == "Voy a revisar el estado,"
    rest = " ".join(chunks[1:]) + " " + " ".join(splitter.flush())
    assert "qué encuentro" in rest


def test_config_save_preserva_claves_desconocidas(tmp_path):
    import json

    path = tmp_path / "voice.json"
    path.write_text(
        json.dumps({"speed": 0.95, "future_key": {"x": 1}}),
        encoding="utf-8",
    )
    cfg = VoiceConfig.load(path)
    cfg.speed = 1.05
    cfg.save(path)
    data = json.loads(path.read_text(encoding="utf-8"))
    assert data["speed"] == pytest.approx(1.05)
    assert data["future_key"] == {"x": 1}
    assert not path.with_name(path.name + ".tmp").exists()


def test_hotkey_parser():
    mods, key = parse_hotkey("ctrl+alt+space")
    assert mods & 0x0002  # Ctrl
    assert mods & 0x0001  # Alt
    assert key == 0x20


def test_strip_ansi_removes_terminal_noise():
    raw = "\x1b[32mhola\x1b[0m\r\n───\r\nmundo"
    assert strip_ansi(raw) == "hola\nmundo"


def test_herdr_list_agents_accepts_string_status():
    cfg = VoiceConfig()
    herdr = Herdr(cfg)
    herdr._json = lambda args, timeout=20: {  # type: ignore[method-assign]
        "terminals": [
            {
                "address": "t1",
                "agent": "claude-code",
                "status": "working",
                "cwd": "C:/proj",
                "where": "proj / 1",
                "turn": 3,
            }
        ]
    }
    agents = herdr.list_agents()
    assert agents[0]["state"] == "working"
    assert agents[0]["agent"] == "claude-code"


def test_watcher_detects_done_and_blocked():
    events: list[tuple] = []
    watcher = Watcher(
        VoiceConfig(),
        on_event=lambda *args: events.append(args),
        should_announce=lambda address: True,
    )
    watcher._compare(
        [{"address": "t1", "agent": "claude-code", "status": "working"}]
    )
    assert events == []
    watcher._compare(
        [{"address": "t1", "agent": "claude-code", "status": "done"}]
    )
    assert events == [("t1", "claude-code", "done", "done")]

    watcher._compare(
        [{"address": "t2", "agent": "opencode", "status": "working"}]
    )
    watcher._compare(
        [{"address": "t2", "agent": "opencode", "status": "blocked"}]
    )
    assert events[-1] == ("t2", "opencode", "blocked", "blocked")


def test_run_agent_ejecuta_una_orden_explicita_sin_confirmacion():
    cfg = VoiceConfig()
    cfg.agents = {"claude": ["claude"]}
    cfg.default_cwd = "C:/proj"
    herdr = Herdr(cfg)
    calls: list[list[str]] = []

    def fake_app(args, timeout=20):
        calls.append(args)
        return {"terminal": "t1"}

    herdr._json_app = fake_app  # type: ignore[method-assign]
    result = herdr.run_agent("claude", "abre una terminal", confirmed=False)
    assert result["ok"] is True
    assert result["terminal"] == "t1"
    assert calls


def test_run_agent_builds_hc_run_and_sends_task():
    cfg = VoiceConfig()
    cfg.agents = {"claude": ["claude"]}
    cfg.default_cwd = "C:/proj"
    herdr = Herdr(cfg)
    calls: list[list[str]] = []

    def fake_json(args, timeout=20):
        calls.append(args)
        return {"address": "t99"}

    herdr._json = fake_json  # type: ignore[method-assign]
    sent: dict = {}

    def fake_send(address, text, confirmed=False):
        sent.update(address=address, text=text, confirmed=confirmed)
        return {"ok": True}

    herdr.send_agent = fake_send  # type: ignore[method-assign]
    result = herdr.run_agent(
        "claude", "haz algo", confirmed=True, background=True
    )
    assert result["ok"] is True
    assert result["address"] == "t99"
    assert result["visible"] is False
    assert "--no-parent" in calls[0]
    assert calls[0][-1] == "claude"
    assert sent == {"address": "t99", "text": "haz algo", "confirmed": True}


def test_run_agent_visible_uses_tab_new_and_gui_skill():
    cfg = VoiceConfig()
    cfg.agents = {"claude": ["claude"]}
    cfg.default_cwd = "C:/proj"
    herdr = Herdr(cfg)
    calls: list[list[str]] = []

    def fake_app(args, timeout=20):
        calls.append(args)
        return {"space": "sp", "tab": "tb", "terminal": "t9"}

    herdr._json_app = fake_app  # type: ignore[method-assign]
    result = herdr.run_agent(
        "claude", "abre Brave y YouTube", confirmed=True, gui=True
    )
    assert result["ok"] is True
    assert result["visible"] is True
    assert result["terminal"] == "t9"
    assert result["background"] is False
    assert calls[0][:2] == ["do", "tab.new"]
    assert "cua-driver" in " ".join(calls[0])


def test_voice_close_tools_and_note_done():
    cfg = VoiceConfig()
    herdr = Herdr(cfg)
    calls: list[tuple] = []
    messenger = Messenger(
        cfg,
        herdr,
        last_summary=lambda: "",
        watch=lambda address: calls.append(("watch", address)),
        mute=lambda minutes: None,
        request_close=lambda: calls.append(("close",)),
        mic_mute=lambda muted: None,
    )
    messenger._execute("voice_close", "{}")
    assert ("close",) in calls
    messenger._execute("voice_close_and_watch", '{"address":"t1"}')
    assert ("watch", "t1") in calls
    assert calls.count(("close",)) == 2

    messenger.note_done("t1", "claude-code", "done", "salida de prueba")
    last = messenger.messages[-1]
    assert last["role"] == "system"
    assert "ha terminado" in last["content"]
    assert "salida de prueba" in last["content"]


def test_voice_close_heuristic_watches_last_agent():
    cfg = VoiceConfig()
    herdr = Herdr(cfg)
    calls: list[tuple] = []
    messenger = Messenger(
        cfg,
        herdr,
        last_summary=lambda: "",
        watch=lambda address: calls.append(("watch", address)),
        mute=lambda minutes: None,
        request_close=lambda: calls.append(("close",)),
        mic_mute=lambda muted: None,
    )
    messenger._last_user_text = "ciérrate y avísame cuando termine"
    messenger._last_agent_address = "t9"
    messenger._execute("voice_close", "{}")
    assert ("watch", "t9") in calls
    assert ("close",) in calls


def test_run_agent_notify_flags_register_watch_and_close():
    cfg = VoiceConfig()
    herdr = Herdr(cfg)
    calls: list[tuple] = []
    messenger = Messenger(
        cfg,
        herdr,
        last_summary=lambda: "",
        watch=lambda address: calls.append(("watch", address)),
        mute=lambda minutes: None,
        request_close=lambda: calls.append(("close",)),
        mic_mute=lambda muted: None,
    )
    messenger.herdr.run_agent = lambda *a, **k: {"ok": True, "address": "t9"}  # type: ignore[method-assign]
    result = messenger._execute(
        "herdr_run_agent",
        '{"agent":"claude","task":"x","confirmed":true,'
        '"notify_on_done":true,"close_listening":true}',
    )
    assert result["ok"] is True
    assert ("watch", "t9") in calls
    assert ("close",) in calls


def test_watcher_respects_mute_filter():
    events: list[tuple] = []
    watcher = Watcher(
        VoiceConfig(),
        on_event=lambda *args: events.append(args),
        should_announce=lambda address: False,
    )
    watcher._compare(
        [{"address": "t1", "agent": "claude-code", "status": "working"}]
    )
    watcher._compare(
        [{"address": "t1", "agent": "claude-code", "status": "done"}]
    )
    assert events == []


def test_config_pinned_respeta_el_proveedor_elegido_en_el_menu(tmp_path, monkeypatch):
    import json

    voice_path = tmp_path / "voice.json"
    llm_path = tmp_path / "llm.json"
    voice_path.write_text(
        json.dumps(
            {
                "llm_pinned": True,
                "llm_provider": "anthropic",
                "llm_model": "claude-sonnet-5-5",
                "llm_base_url": "https://api.anthropic.com/v1/",
                "api_key": "",
            }
        ),
        encoding="utf-8",
    )
    llm_path.write_text(
        json.dumps(
            {
                "provider": "groq",
                "providers": {
                    "anthropic": {
                        "base_url": "https://api.anthropic.com/v1/",
                        "model": "claude-sonnet-5-5",
                        "api_key": "sk-ant-guardada",
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    monkeypatch.setenv("LLM_PROVIDER", "groq")
    monkeypatch.setenv("GROQ_API_KEY", "gsk-del-entorno")
    monkeypatch.setenv("EZ_LLM_CONFIG", str(llm_path))
    cfg = VoiceConfig.load(voice_path)
    assert cfg.llm_provider == "anthropic"
    assert cfg.llm_model == "claude-sonnet-5-5"
    assert cfg.api_key == "sk-ant-guardada"


def test_catalogo_local_de_modelos_whisper(tmp_path, monkeypatch):
    from ezagent.voice.stt import available_whisper_models, whisper_model_path

    models = tmp_path / "models"
    models.mkdir()
    (models / "ggml-small-q5_1.bin").write_bytes(b"x")
    (models / "ggml-large-v3-turbo-q5_0.bin").write_bytes(b"x")
    monkeypatch.setenv("DICTA_HOME", str(tmp_path))
    options = available_whisper_models("ggml-large-v3-turbo-q5_0")
    assert [value for value, _label in options] == [
        "ggml-small-q5_1",
        "ggml-large-v3-turbo-q5_0",
    ]
    assert whisper_model_path("ggml-small-q5_1") == models / "ggml-small-q5_1.bin"
    assert whisper_model_path("no-existe") is None


def test_ensure_whisper_elige_puerto_local_libre(monkeypatch):
    from ezagent.voice import app as app_module

    cfg = VoiceConfig(
        whisper_url="http://127.0.0.1:8088/inference",
        auto_start_whisper=True,
        stt_model="ggml-small-q5_1",
    )
    calls = {"ready": 0}

    def fake_ready(url, timeout=1.0):
        calls["ready"] += 1
        return calls["ready"] > 1

    captured = {}

    class FakeProc:
        pid = 123

        def poll(self):
            return None

    def fake_popen(command, **kwargs):
        captured["command"] = command
        return FakeProc()

    monkeypatch.setattr(app_module, "_whisper_ready", fake_ready)
    monkeypatch.setattr(app_module, "_free_local_port", lambda: 51234)
    monkeypatch.setattr(app_module, "whisper_model_path", lambda model: None)
    monkeypatch.setattr(app_module.subprocess, "Popen", fake_popen)

    proc = app_module.ensure_whisper(cfg, wait_secs=0.1)
    assert proc is not None
    assert cfg.whisper_url == "http://127.0.0.1:51234/inference"
    command = captured["command"]
    assert "51234" in command
    assert "auto" in command
