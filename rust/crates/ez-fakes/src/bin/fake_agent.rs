//! Binario `fake-agent`: imprime una pantalla de agente guionizada (F1-11).

use std::io::Write;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

const USAGE: &str = "\
fake-agent: programa de terminal guionizado con pantallas de agente.

Uso:
  fake-agent [--scenario RUTA] [--agent NOMBRE] [--task TEXTO]
             [--conclusion TEXTO] [--scene ESCENA]... [--delay-ms N]

Opciones:
  --scenario RUTA    Escenario JSON (agent, task, conclusion, script, delay_ms).
  --agent NOMBRE     opencode (defecto), claude, codex o mcode.
  --task TEXTO       Tarea que se ve en la pantalla.
  --conclusion TEXTO Texto final cuando la escena es «done».
  --scene ESCENA     working, done, blocked, usage_limit o context_full;
                     se puede repetir y sustituye al guion del escenario.
  --delay-ms N       Pausa entre pantallas (defecto: el del escenario).

Ejemplo:
  fake-agent --scenario examples/fake_agent.json --agent claude
";

#[derive(Debug, Default)]
struct Args {
    scenario: Option<String>,
    agent: Option<String>,
    task: Option<String>,
    conclusion: Option<String>,
    scenes: Vec<String>,
    delay_ms: Option<u64>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{} necesita un valor", args[index]))?;
        match args[index].as_str() {
            "--scenario" => parsed.scenario = Some(value.clone()),
            "--agent" => parsed.agent = Some(value.clone()),
            "--task" => parsed.task = Some(value.clone()),
            "--conclusion" => parsed.conclusion = Some(value.clone()),
            "--scene" => parsed.scenes.push(value.clone()),
            "--delay-ms" => {
                parsed.delay_ms = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("--delay-ms no es un número: {value}"))?,
                );
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
        index += 2;
    }
    Ok(parsed)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let parsed = match parse(&args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("fake-agent: {error}");
            return ExitCode::from(2);
        }
    };
    let result = (|| -> anyhow::Result<()> {
        let mut scenario = match parsed.scenario.as_deref() {
            Some(path) => ez_fakes::fake_agent::Scenario::load(std::path::Path::new(path))?,
            None => ez_fakes::fake_agent::Scenario::default(),
        };
        if let Some(agent) = parsed.agent {
            scenario.agent = agent;
        }
        if let Some(task) = parsed.task {
            scenario.task = task;
        }
        if let Some(conclusion) = parsed.conclusion {
            scenario.conclusion = conclusion;
        }
        if !parsed.scenes.is_empty() {
            scenario.script = parsed.scenes;
        }
        let delay = parsed.delay_ms.unwrap_or(scenario.delay_ms);
        for frame in scenario.frames() {
            println!("{frame}");
            std::io::stdout().flush()?;
            if delay > 0 {
                sleep(Duration::from_millis(delay));
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fake-agent: {error}");
            ExitCode::FAILURE
        }
    }
}
