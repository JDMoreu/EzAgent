//! Ida y vuelta `ez` ↔ servidor con un `Handler` falso.

mod support;

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use support::{Harness, FISH_TOKEN, SPACE, TASK, TOKEN};

use ez_ctl::Caller;

#[test]
fn leaf_commands_use_their_protocol_names() {
    let mut harness = Harness::new();
    let argv: [&[&str]; 6] = [
        &["status"],
        &["verify"],
        &["round", "start"],
        &["round", "close"],
        &["close"],
        &["pause", "--reason", "Necesito una decisión"],
    ];
    for args in argv {
        let (code, out, err) = harness.run(args);
        assert_eq!(code, 0, "{}: {err}", args.join(" "));
        assert!(out.starts_with("hecho: "), "{}", out);
    }
    let calls = harness.handler.calls();
    let names: Vec<&str> = calls
        .iter()
        .map(|(_, request)| request.command.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "status",
            "verify",
            "round.start",
            "round.close",
            "task.close",
            "pause"
        ]
    );
    assert_eq!(
        calls[0].0,
        Caller::Elephant {
            space: SPACE.to_string()
        }
    );
    assert_eq!(calls[0].1.args, json!({}));
    assert_eq!(
        calls[5].1.args,
        json!({ "reason": "Necesito una decisión" })
    );
    harness.stop();
}

#[test]
fn file_commands_send_absolute_paths() {
    let mut harness = Harness::new();
    let brief = harness.dir.file("encargo.md");
    let criteria = harness.dir.file("criterios.toml");
    let team = harness.dir.file("equipo.toml");
    let report = harness.dir.file("informe.md");
    let brief_path = brief.to_string_lossy().into_owned();
    let criteria_path = criteria.to_string_lossy().into_owned();
    let team_path = team.to_string_lossy().into_owned();
    let report_path = report.to_string_lossy().into_owned();

    let cases: [(&[&str], &str); 4] = [
        (
            &[
                "task",
                "new",
                "--title",
                "Título",
                "--brief",
                "encargo.md",
                "--criteria",
                "criterios.toml",
            ],
            "task.new",
        ),
        (&["team", "propose", "--file", &team_path], "team.propose"),
        (&["fish", "developer", "--brief", "encargo.md"], "fish.new"),
        (&["send", "dev-r1-1", "--file", "informe.md"], "fish.send"),
    ];
    for (args, command) in cases {
        let (code, _, err) = harness.run(args);
        assert_eq!(code, 0, "{command}: {err}");
    }

    let calls = harness.handler.calls();
    assert_eq!(
        calls[0].1.args,
        json!({
            "title": "Título",
            "brief": brief_path,
            "criteria": criteria_path,
        })
    );
    assert_eq!(calls[1].1.args, json!({ "file": team_path }));
    assert_eq!(
        calls[2].1.args,
        json!({
            "role": "developer",
            "brief": brief_path,
        })
    );
    assert_eq!(
        calls[3].1.args,
        json!({
            "fish": "dev-r1-1",
            "file": report_path,
        })
    );
    harness.stop();
}

#[test]
fn inventory_and_wait_carry_their_arguments() {
    let mut harness = Harness::new();
    let (code, out, err) = harness.run(&["inventory"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.trim(), "hecho: inventory");

    let (code, out, err) = harness.run(&["inventory", "--json"]);
    assert_eq!(code, 0, "{err}");
    let reply: Value = serde_json::from_str(out.trim()).expect("respuesta json");
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["message"], "hecho: inventory");
    assert_eq!(reply["data"]["eco"], "inventory");

    assert_eq!(harness.run(&["wait"]).0, 0);
    assert_eq!(harness.run(&["wait", "--timeout", "7"]).0, 0);

    let calls = harness.handler.calls();
    assert_eq!(calls[0].1.args, json!({ "json": false }));
    assert_eq!(calls[1].1.args, json!({ "json": true }));
    assert_eq!(calls[2].1.args, json!({ "timeout": 170 }));
    assert_eq!(calls[3].1.args, json!({ "timeout": 7 }));
    harness.stop();
}

#[test]
fn report_carries_verdict_counts_and_mutations() {
    let mut harness = Harness::new();
    let _report = harness.dir.file("informe.md");
    let (code, _, err) = harness.run_as(
        FISH_TOKEN,
        &[
            "report",
            "--verdict",
            "pass",
            "--file",
            "informe.md",
            "--blocking",
            "1",
            "--major",
            "2",
            "--minor",
            "3",
            "--mutations-tried",
            "4",
            "--mutations-caught",
            "4",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = harness.run_as(
        FISH_TOKEN,
        &["report", "--verdict", "fail", "--file", "informe.md"],
    );
    assert_eq!(code, 0, "{err}");

    let calls = harness.handler.calls();
    assert_eq!(
        calls[0].0,
        Caller::Fish {
            space: SPACE.to_string(),
            task: TASK.to_string(),
            fish: "dev-r1-1".to_string(),
            attempt: 1,
        }
    );
    let file = calls[0].1.args["file"].as_str().expect("ruta");
    assert!(file.ends_with("informe.md"), "{file}");
    assert_eq!(
        calls[0].1.args,
        json!({
            "verdict": "pass",
            "file": file,
            "blocking": 1,
            "major": 2,
            "minor": 3,
            "mutations_tried": 4,
            "mutations_caught": 4,
        })
    );
    assert_eq!(calls[1].1.args["verdict"], "fail");
    assert_eq!(calls[1].1.args["blocking"], 0);
    assert_eq!(calls[1].1.args["major"], 0);
    assert_eq!(calls[1].1.args["minor"], 0);
    assert_eq!(calls[1].1.args["mutations_tried"], Value::Null);
    assert_eq!(calls[1].1.args["mutations_caught"], Value::Null);
    harness.stop();
}

#[test]
fn a_long_wait_does_not_block_other_commands() {
    let mut harness = Harness::new();
    harness.handler.delay("wait", Duration::from_secs(2));
    let url = harness.url.clone();
    let cwd = harness.dir.path().to_path_buf();
    let slow = std::thread::spawn(move || support::run_at(&url, TOKEN, &cwd, &["wait"]));

    std::thread::sleep(Duration::from_millis(200));
    let start = Instant::now();
    let (code, _, err) = harness.run(&["status"]);
    let elapsed = start.elapsed();
    assert_eq!(code, 0, "{err}");
    assert!(
        elapsed < Duration::from_millis(1500),
        "status tardó {elapsed:?} con un wait en curso"
    );
    assert!(!slow.is_finished(), "el wait ya había terminado");
    let (code, _, err) = slow.join().expect("hilo del wait");
    assert_eq!(code, 0, "{err}");
    harness.stop();
}
