use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use super::*;

struct Fake {
    close_action: &'static str,
    last_summary: &'static str,
    pause_message: &'static str,
    tasks: Vec<Map<String, Value>>,
    labels: Vec<(String, String)>,
    ack: RefCell<Option<(f64, Option<String>, bool)>>,
    ack_index: Cell<usize>,
}

impl Default for Fake {
    fn default() -> Self {
        Self {
            close_action: "pause",
            last_summary: "Resumen anterior.",
            pause_message: "En pausa. Di «oye ez» cuando me necesites.",
            tasks: Vec::new(),
            labels: Vec::new(),
            ack: RefCell::new(None),
            ack_index: Cell::new(0),
        }
    }
}

impl FastHost for Fake {
    fn close_action(&self) -> String {
        self.close_action.to_string()
    }

    fn last_summary(&self) -> String {
        self.last_summary.to_string()
    }

    fn pause_message(&self) -> String {
        self.pause_message.to_string()
    }

    fn pending_tasks(&self) -> Vec<Map<String, Value>> {
        self.tasks.clone()
    }

    fn label_for(&self, address: &str) -> String {
        self.labels
            .iter()
            .find(|(key, _)| key == address)
            .map(|(_, label)| label.clone())
            .unwrap_or_default()
    }

    fn ack_context(&self) -> Option<(f64, Option<String>, bool)> {
        self.ack.borrow_mut().take()
    }

    fn ack_index(&self) -> usize {
        let index = self.ack_index.get();
        self.ack_index.set(index + 1);
        index
    }
}

struct Defaults;

impl FastHost for Defaults {}

fn run(text: &str) -> FastOutcome {
    fast_command(text, &Fake::default())
}

fn run_with(text: &str, host: &Fake) -> FastOutcome {
    fast_command(text, host)
}

fn speech(outcome: &FastOutcome) -> Option<&str> {
    outcome.speech.as_ref().map(|(_, text)| text.as_str())
}

fn map(pairs: &[(&str, &str)]) -> Map<String, Value> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
        .collect()
}

fn task(address: &str, agent: &str, text: &str) -> Map<String, Value> {
    map(&[("address", address), ("agent", agent), ("task", text)])
}

#[test]
fn el_agradecimiento_solo_cuando_todo_es_cortesia() {
    for text in ["gracias", "muchas gracias", "ok gracias", "Gracias 🙂"] {
        let outcome = run(text);
        assert!(outcome.handled, "{text}");
        assert_eq!(outcome.speech, Some(("answer", "De nada.".to_string())));
        assert_eq!(outcome.mic, None);
    }
    // «gracias, mutéate» no es solo cortesía: manda el atajo de mute.
    assert_eq!(run("gracias, mutéate").mic, Some(true));
}

#[test]
fn el_saludo_responde_sin_pasar_por_el_llm() {
    for text in ["hola", "buenas ezagent", "HOLA 🙂"] {
        let outcome = run(text);
        assert!(outcome.handled, "{text}");
        assert_eq!(speech(&outcome), Some("Hola, ¿qué necesitas?"));
    }
}

#[test]
fn silenciar_exige_nombrar_el_microfono() {
    for text in [
        "mutéate",
        "Mutea el micrófono.",
        "silencia el micro",
        "silencia el micrófono",
        "silencio el micro",
        "apaga el micrófono",
        "silencia mi micro",
        "mutea",
    ] {
        let outcome = run(text);
        assert!(outcome.handled, "{text}");
        assert_eq!(outcome.mic, Some(true), "{text}");
        assert_eq!(
            speech(&outcome),
            Some("En pausa. Di «oye ez» cuando me necesites."),
            "{text}"
        );
    }
    // DECISION del Python: «silencio» o «mute» sueltos ya no silencian.
    assert!(!run("silencio").handled);
    assert!(!run("mute").handled);
}

#[test]
fn reactivar_el_microfono() {
    for text in [
        "desmutea",
        "activa el micrófono",
        "unmute",
        "reactiva el micro",
        "desmute",
    ] {
        let outcome = run(text);
        assert!(outcome.handled, "{text}");
        assert_eq!(outcome.mic, Some(false), "{text}");
        assert_eq!(speech(&outcome), Some("Micrófono activo."));
    }
}

#[test]
fn cerrar_respeta_el_ajuste_close_action() {
    let outcome = run("cierra la escucha");
    assert_eq!(outcome.close, Some(CloseMode::Pause));
    assert_eq!(
        speech(&outcome),
        Some("En pausa. Di «oye ez» cuando me necesites.")
    );

    let hangup = Fake {
        close_action: "hangup",
        ..Fake::default()
    };
    let outcome = run_with("cierra la escucha", &hangup);
    assert_eq!(outcome.close, Some(CloseMode::Hangup));
    assert_eq!(speech(&outcome), Some("Listo, cierro."));

    let odd = Fake {
        close_action: "PAUSA",
        ..Fake::default()
    };
    assert_eq!(run_with("pausa", &odd).close, Some(CloseMode::Pause));
    let upper = Fake {
        close_action: "HANGUP",
        ..Fake::default()
    };
    assert_eq!(run_with("pausa", &upper).close, Some(CloseMode::Hangup));
}

#[test]
fn despedirse_o_pedir_colgar_cuelga_siempre() {
    for text in ["adiós", "cuelga", "termina la llamada", "colgar"] {
        let outcome = run(text);
        assert_eq!(outcome.close, Some(CloseMode::Hangup), "{text}");
        assert_eq!(speech(&outcome), Some("Listo, cierro."), "{text}");
    }
    assert_eq!(run("colgar").close, Some(CloseMode::Hangup));
}

#[test]
fn repetir_dice_lo_ultimo_o_avisa() {
    assert_eq!(speech(&run("repítelo")), Some("Resumen anterior."));
    assert_eq!(speech(&run("otra vez")), Some("Resumen anterior."));
    let empty = Fake {
        last_summary: "",
        ..Fake::default()
    };
    assert_eq!(
        speech(&run_with("repite", &empty)),
        Some("No hay nada que repetir todavía.")
    );
}

#[test]
fn lo_que_no_es_atajo_sigue_al_llm() {
    for text in [
        "",
        "haz algo",
        "abre el archivo",
        "dame un momento",
        "hola mundo",
        "qué agentes hay en la oficina",
    ] {
        let outcome = run(text);
        assert!(!outcome.handled, "{text}");
        assert_eq!(outcome, FastOutcome::default(), "{text}");
    }
}

#[test]
fn listar_agentes_delega_en_el_hilo_de_trabajo() {
    let outcome = run("qué agentes hay");
    assert!(outcome.handled);
    assert!(outcome.list_agents);
    assert!(outcome.speech.is_none());
    assert!(!outcome.reveal_activity);
    assert_eq!(LIST_AGENTS_THREAD, "ezagent-list-agents");
}

#[test]
fn el_reporte_de_agentes_usa_etiqueta_y_estado() {
    let agents = [
        map(&[("label", "Claude Code"), ("state", "working")]),
        map(&[("label", "Codex"), ("state", "done")]),
        map(&[("label", "OpenCode"), ("state", "")]),
    ];
    assert_eq!(
        agents_report(&agents),
        "Hay 3 agentes: Claude Code, trabajando; Codex, terminado; OpenCode, —."
    );
    assert_eq!(agents_report(&[]), "No hay agentes vivos.");
    assert_eq!(AGENTS_UNAVAILABLE, "No pude consultar Herdr.");
}

#[test]
fn el_reporte_de_agentes_topa_en_cuatro_y_suma_el_resto() {
    let agents: Vec<Map<String, Value>> = (0..6)
        .map(|index| map(&[("label", &format!("Agente {index}")), ("state", "idle")]))
        .collect();
    assert!(agents_report(&agents).ends_with("Agente 3, inactivo, y 2 más."));
    assert_eq!(
        agents_report(&[map(&[("label", "X"), ("state", "raro")])]),
        "Hay 1 agentes: X, raro."
    );
}

#[test]
fn sin_etiqueta_visible_se_usa_visible_name() {
    let agents = [
        map(&[("where", "EzAgent / Claude Code"), ("state", "working")]),
        map(&[("space", "EzAgent"), ("name", "2"), ("state", "done")]),
        map(&[
            ("agent", "claude-code"),
            ("cwd", "C:\\repo\\demo"),
            ("state", "blocked"),
        ]),
        map(&[("agent", ""), ("state", "idle")]),
    ];
    assert_eq!(
        agents_report(&agents),
        "Hay 4 agentes: Claude Code de EzAgent, trabajando; pestaña 2 de EzAgent, terminado; \
         Claude Code en demo, bloqueado; el agente, inactivo."
    );
}

#[test]
fn listar_tareas_sin_pendientes() {
    let outcome = run("qué tareas pendientes");
    assert!(outcome.handled);
    assert!(outcome.reveal_activity);
    assert_eq!(speech(&outcome), Some("No tengo tareas pendientes."));
    assert_eq!(outcome.mic, None);
    assert!(!outcome.list_agents);
}

#[test]
fn listar_tareas_recorta_y_nombra_cada_una() {
    let long = format!("arreglar {}", "x".repeat(120));
    let host = Fake {
        tasks: vec![
            task("t1", "codex", "revisar los tests"),
            task("t2", "claude", &long),
        ],
        ..Fake::default()
    };
    let expected = format!(
        "Tengo 2 tareas pendientes: codex: revisar los tests; claude: arreglar {}….",
        "x".repeat(90)
    );
    assert_eq!(
        speech(&run_with("cómo van las tareas", &host)),
        Some(expected.as_str())
    );
}

#[test]
fn listar_tareas_prefiere_la_etiqueta_de_herdr() {
    let host = Fake {
        tasks: vec![
            task("t1", "codex", "revisar"),
            task("t2", "", "  arreglar   esto "),
        ],
        labels: vec![("t1".to_string(), "Pestaña 1".to_string())],
        ..Fake::default()
    };
    assert_eq!(
        speech(&run_with("tareas pendientes", &host)),
        Some("Tengo 2 tareas pendientes: Pestaña 1: revisar; agente: arreglar esto.")
    );
}

#[test]
fn listar_tareas_topa_en_cuatro_y_rellena_la_descripcion() {
    let tasks: Vec<Map<String, Value>> = (0..6)
        .map(|index| task(&format!("t{index}"), "codex", ""))
        .collect();
    let host = Fake {
        tasks,
        ..Fake::default()
    };
    assert_eq!(
        speech(&run_with("qué tareas tienes", &host)),
        Some(
            "Tengo 6 tareas pendientes: codex: sin descripción; codex: sin descripción; \
             codex: sin descripción; codex: sin descripción; y 2 más."
        )
    );
}

#[test]
fn una_descripcion_de_cien_caracteres_no_se_recorta() {
    let host = Fake {
        tasks: vec![task("t1", "codex", &"x".repeat(100))],
        ..Fake::default()
    };
    let expected = format!("Tengo 1 tareas pendientes: codex: {}….", "x".repeat(99));
    let host101 = Fake {
        tasks: vec![task("t1", "codex", &"x".repeat(101))],
        ..Fake::default()
    };
    assert_eq!(
        speech(&run_with("qué tareas tienes", &host101)),
        Some(expected.as_str())
    );
    let outcome = run_with("qué tareas tienes", &host);
    let text = speech(&outcome).unwrap_or_default();
    assert!(text.contains(&"x".repeat(100)));
}

#[test]
fn las_cortesias_iniciales_se_quitan_antes_del_atajo() {
    assert_eq!(strip_courtesy(""), "");
    assert_eq!(strip_courtesy("ok"), "ok");
    assert_eq!(strip_courtesy("ok mutéate"), "mutéate");
    assert_eq!(
        strip_courtesy("Ok, gracias. Mutéate"),
        "Ok, gracias. Mutéate"
    );
    assert_eq!(strip_courtesy("ok gracias mutéate"), "mutéate");
    assert_eq!(strip_courtesy("va"), "va");
    assert_eq!(strip_courtesy("muchas gracias"), "muchas gracias");
    assert_eq!(strip_courtesy("eh este pues va"), "eh este pues va");
    assert_eq!(
        strip_courtesy("listo cierra la escucha"),
        "cierra la escucha"
    );
    assert_eq!(strip_courtesy("ok   mutéate"), "mutéate");
    assert_eq!(strip_courtesy("ok\tmutéate"), "mutéate");
    assert_eq!(strip_courtesy("OK mutéate"), "OK mutéate");
    assert_eq!(strip_courtesy("ok ñandú"), "ñandú");
    assert_eq!(strip_courtesy("muchas gracias cierra"), "cierra");
    assert_eq!(strip_courtesy("  ok  mutéate  "), "mutéate");
    assert_eq!(strip_courtesy("gracias por todo"), "por todo");
    assert_eq!(strip_courtesy("este es el bueno"), "es el bueno");

    assert_eq!(strip_courtesy("ok cierra la escucha"), "cierra la escucha");
    assert_eq!(
        run("ok, gracias. mutéate").mic,
        Some(true),
        "las cortesías se quitan al principio"
    );
}

#[test]
fn normaliza_como_el_python() {
    assert_eq!(normalize("Ok, gracias. Mutéate"), "ok gracias muteate");
    assert_eq!(normalize("Mutea el micrófono."), "mutea el microfono");
    assert_eq!(normalize("cuántos agentes hay"), "cuantos agentes hay");
    assert_eq!(normalize("HOLA 🙂"), "hola");
    assert_eq!(normalize(""), "");
    assert_eq!(normalize("a,,,b"), "a b");
}

#[test]
fn los_conjuntos_de_atajos_no_tienen_duplicados() {
    let sets: [&[&str]; 8] = [
        &GREET_WORDS,
        &AGENT_WORDS,
        &TASK_WORDS,
        &MUTE_WORDS,
        &UNMUTE_WORDS,
        &CLOSE_WORDS,
        &HANGUP_WORDS,
        &REPEAT_WORDS,
    ];
    for set in sets {
        let unique: HashSet<&&str> = set.iter().collect();
        assert_eq!(unique.len(), set.len());
    }
}

#[test]
fn los_valores_por_defecto_del_host_son_los_de_fabrica() {
    let host = Defaults;
    assert_eq!(host.close_action(), "pause");
    assert_eq!(host.last_summary(), "");
    assert_eq!(
        host.pause_message(),
        "En pausa. Di «EzAgent, escúchame» cuando me necesites."
    );
    assert_eq!(host.agent_label("codex"), "codex");
    assert_eq!(host.agent_label(""), "El agente");
    assert!(host.pending_tasks().is_empty());
    assert_eq!(host.label_for("t1"), "");
    assert!(host.ack_context().is_none());
    assert_eq!(host.ack_index(), 0);
    assert_eq!(host.ack_window_secs(), ACK_WINDOW_SECS);
}

fn ack_host(seconds: f64, kind: &str, question: bool) -> Fake {
    let host = Fake::default();
    *host.ack.borrow_mut() = Some((seconds, Some(kind.to_string()), question));
    host
}

#[test]
fn el_acuse_corto_responde_sin_pasar_por_el_llm() {
    // «gracias» y «ok gracias» los gana el atajo de agradecimiento («De nada.»).
    for text in [
        "ok",
        "okey",
        "oki",
        "oc",
        "k",
        "va",
        "vale",
        "bueno",
        "listo",
        "perfecto",
        "dale",
        "genial",
        "de acuerdo",
        "ya está",
        "muy bien",
        "esta bien",
        "entendido",
        "confirmado",
        "ok ok",
        "vale vale",
    ] {
        let outcome = run_with(text, &ack_host(1.0, "announce", false));
        assert!(outcome.handled, "{text}");
        assert!(outcome.ack, "{text}");
        assert_eq!(
            outcome.speech.as_ref().map(|(kind, _)| *kind),
            Some("ack"),
            "{text}"
        );
        assert!(
            ACK_REPLIES.contains(&speech(&outcome).unwrap_or_default()),
            "{text}: {:?}",
            speech(&outcome)
        );
    }
    // El agradecimiento suelto conserva su atajo de siempre.
    for text in ["gracias", "ok gracias", "muchas gracias"] {
        let outcome = run_with(text, &ack_host(1.0, "announce", false));
        assert_eq!(speech(&outcome), Some("De nada."), "{text}");
        assert!(!outcome.ack, "{text}");
    }
}

#[test]
fn el_acuse_no_aplica_sin_salida_reciente_o_tras_pregunta() {
    // Sin contexto, tras confirmación, fuera de ventana o tras pregunta: al LLM.
    for host in [
        Fake::default(),
        ack_host(1.0, "confirm", false),
        ack_host(11.0, "announce", false),
        ack_host(1.0, "announce", true),
        ack_host(-1.0, "answer", false),
    ] {
        let outcome = run_with("ok", &host);
        assert!(!outcome.handled);
        assert!(!outcome.ack);
        assert_eq!(outcome, FastOutcome::default());
    }
    // Las respuestas habladas por la app también cuentan como salida.
    let outcome = run_with("ok", &ack_host(1.0, "answer", false));
    assert!(outcome.handled);
    assert_eq!(outcome.speech.as_ref().map(|(kind, _)| *kind), Some("ack"));
}

#[test]
fn las_respuestas_de_acuse_rotan_sin_repetir_la_anterior() {
    struct Rotating(Cell<usize>);
    impl FastHost for Rotating {
        fn ack_context(&self) -> Option<(f64, Option<String>, bool)> {
            Some((1.0, Some("announce".to_string()), false))
        }
        fn ack_index(&self) -> usize {
            let index = self.0.get();
            self.0.set(index + 1);
            index
        }
    }
    let host = Rotating(Cell::new(0));
    let replies: Vec<String> = (0..8)
        .map(|_| {
            let outcome = fast_command("ok", &host);
            speech(&outcome).unwrap_or_default().to_string()
        })
        .collect();
    for (reply, expected) in replies.iter().zip(ACK_REPLIES.iter().cycle()) {
        assert_eq!(reply, expected);
    }
    assert_eq!(replies[6], replies[0]);
    assert_eq!(replies[7], replies[1]);
    for pair in replies.windows(2) {
        assert_ne!(pair[0], pair[1]);
    }
}

#[test]
fn un_atajo_real_no_consume_el_contexto_de_acuse() {
    let host = ack_host(1.0, "announce", false);
    // «ok, mutea» sigue silenciando y no gasta el aviso reciente.
    assert_eq!(run_with("ok mutea", &host).mic, Some(true));
    assert!(run_with("ok", &host).ack);
}

#[test]
fn clausula_de_acuse_rechaza_texto_con_contenido_nuevo() {
    for text in [
        "ok, pero cambia X",
        "listo haz esto",
        "hola mundo",
        "sí",
        "no",
    ] {
        assert!(!is_pure_acknowledgement(&normalize(text)), "{text}");
        let outcome = run_with(text, &ack_host(1.0, "announce", false));
        assert!(!outcome.handled, "{text}");
    }
}
