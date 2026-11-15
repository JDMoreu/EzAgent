//! Paridad de `tools_skills` contra `parity/fixtures/tools_skills.jsonl` (P-AGENT-C).
//!
//! Un solo `#[test]`: los casos comparten el directorio temporal. El intérprete
//! y `subprocess.run` se sustituyen como en el oráculo de Python.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ez_agent::registry::{ToolInfo, ToolResult, ToolSpec};
use ez_agent::skills::SkillLibrary;
use ez_agent::tools_d::{Completed, RunError};
use ez_agent::tools_skills::{
    build_skill_tools_with_hooks, ScriptCall, ScriptRunner, SkillHooks, SCRIPT_TIMEOUT_S,
};
use ez_core::agent_config::AgentConfig;
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/tools_skills.jsonl";
const INTERPRETER: &str = "C:/Python/python.exe";

type SkillFile = (&'static str, &'static [u8]);
type SkillFixture = (&'static str, &'static str, &'static [SkillFile]);

const SKILLS: &[SkillFixture] = &[
    (
        "demo",
        "---\nname: demo\ndescription: Skill de prueba para scripts.\n---\nCuerpo de la demo.\n\nSegunda línea.\n",
        &[
            ("scripts/ok.py", b"print('ok')\n"),
            ("scripts/fail.py", b"raise SystemExit(2)\n"),
            ("scripts/rc3.py", b"raise SystemExit(3)\n"),
            ("scripts/big.py", b"print('x')\n"),
            ("scripts/slow.py", b"import time\n"),
            ("scripts/slowstr.py", b"import time\n"),
            ("scripts/boom.py", b"print('boom')\n"),
            ("scripts/empty.py", b"pass\n"),
            ("scripts/MAYUS.PY", b"print('mayus')\n"),
            ("scripts/note.txt", b"no es python\n"),
            ("references/guia.md", b"Gu\xc3\xada de la skill.\n"),
            ("references/crlf.md", b"uno\r\ndos\r\n"),
            ("references/grande.md", &[b'h'; 20010]),
            ("assets/binario.dat", b"\xff\xfe roto"),
        ],
    ),
    (
        "sin-scripts",
        "---\nname: sin-scripts\ndescription: Skill sin carpeta scripts.\n---\nSin scripts.\n",
        &[("references/guia.md", b"guia\n")],
    ),
    (
        "vacia",
        "---\nname: vacia\ndescription: Skill con cuerpo vacío.\n---\n   \n\n",
        &[],
    ),
    (
        "grande",
        "---\nname: grande\ndescription: Cuerpo enorme.\n---\n",
        &[],
    ),
    (
        "crlf",
        "---\r\nname: crlf\r\ndescription: Editada en Windows.\r\n---\r\nLínea uno.\r\nLínea dos.\r\n",
        &[],
    ),
];

fn write_tree(skills_dir: &Path) {
    for (folder, skill_md, files) in SKILLS {
        let root = skills_dir.join(folder);
        std::fs::create_dir_all(&root).expect("skill");
        let body = if *folder == "grande" {
            format!("{skill_md}{}", "g".repeat(20010))
        } else {
            (*skill_md).to_string()
        };
        std::fs::write(root.join("SKILL.md"), body.as_bytes()).expect("SKILL.md");
        for (relative, content) in files.iter() {
            let target = root.join(relative);
            std::fs::create_dir_all(target.parent().expect("carpeta")).expect("carpeta");
            let bytes = if *relative == "references/grande.md" {
                vec![b'h'; 20010]
            } else {
                content.to_vec()
            };
            std::fs::write(target, bytes).expect("archivo");
        }
    }
}

fn config(work: &Path, allow_scripts: bool) -> AgentConfig {
    let (mut config, _) = AgentConfig::load(Some(&work.join("app").join("permissions.toml")));
    config.app_dir = work.join("app");
    config.workspace = work.join("workspace");
    config.skills_dir = work.join("skills");
    config.global_skills_dir = work.join("global-skills");
    config.journal_dir = work.join("app").join("journal");
    config.skills.allow_scripts = allow_scripts;
    config
}

fn library(config: &AgentConfig) -> Arc<SkillLibrary> {
    Arc::new(SkillLibrary::new(
        &config.skills_dir,
        &config.global_skills_dir,
        None,
        200,
    ))
}

struct FakeRunner {
    calls: Mutex<Vec<Value>>,
}

impl ScriptRunner for FakeRunner {
    fn run(&self, call: &ScriptCall<'_>) -> Result<Completed, RunError> {
        let basename = Path::new(&call.argv[1])
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut env_keys: Vec<&str> = call.env.iter().map(|(key, _)| key.as_str()).collect();
        env_keys.sort_unstable();
        let mut shown = call.argv.to_vec();
        if let Some(second) = shown.get_mut(1) {
            *second = basename.clone();
        }
        self.calls.lock().expect("llamadas").push(json!({
            "argv": shown,
            "argv_full": call.argv,
            "shell": false,
            "cwd": call.cwd.display().to_string(),
            "timeout": call.timeout_s,
            "encoding": "utf-8",
            "errors": "replace",
            "text": true,
            "capture_output": true,
            "check": false,
            "env_keys": env_keys,
        }));
        match basename.as_str() {
            "slow.py" => Err(RunError::Timeout {
                stdout: "parcial\n".to_string(),
            }),
            "slowstr.py" => Err(RunError::Timeout {
                stdout: "texto\n".to_string(),
            }),
            "boom.py" => Err(RunError::Os("no se encontró el intérprete".to_string())),
            "ok.py" => Ok(done(0, "hola desde el script\n", "")),
            "MAYUS.PY" => Ok(done(0, "mayus\n", "")),
            "fail.py" => Ok(done(2, "algo\n", "  falló  \n")),
            "rc3.py" => Ok(done(3, "", "   \n")),
            "big.py" => Ok(done(0, &"z".repeat(9000), &"e".repeat(9000))),
            "empty.py" => Ok(Completed {
                stdout: String::new(),
                stderr: String::new(),
                returncode: 0,
            }),
            _ => Ok(done(0, "", "")),
        }
    }
}

fn done(code: i64, stdout: &str, stderr: &str) -> Completed {
    Completed {
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        returncode: code,
    }
}

fn approvals_text(mode: &str, hash: &str) -> Option<String> {
    match mode {
        "missing" => None,
        "match" => Some(format!("{{\n  \"demo\": \"{hash}\"\n}}\n")),
        "stale" => Some("{\n  \"demo\": \"hash-viejo\"\n}\n".to_string()),
        "corrupt" => Some("{no es json".to_string()),
        "non-object" => Some("[\"lista\"]".to_string()),
        _ => Some("{\n  \"demo\": 5,\n  \"otra\": \"x\"\n}\n".to_string()),
    }
}

fn tool_info(info: &ToolInfo) -> Value {
    json!({
        "level": info.level,
        "bulk": info.bulk,
        "always_confirm": info.always_confirm,
        "summary": info.summary,
    })
}

fn normalize(value: &Value, temp: &str) -> Value {
    match value {
        Value::String(text) => Value::String(
            text.replace(temp, "<tmp>")
                .replace(temp.replace('\\', "/").as_str(), "<tmp>")
                .replace('\\', "/"),
        ),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| normalize(item, temp)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), normalize(item, temp)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn call_result(tool: &ToolSpec, args: &serde_json::Map<String, Value>) -> Value {
    match (tool.handler)(args) {
        Ok(ez_agent::registry::ToolReturn::Result(result)) => result.to_dict(),
        Ok(ez_agent::registry::ToolReturn::Value(value)) => value,
        Err(error) => ToolResult::failed(error).to_dict(),
    }
}

fn run_call(case: &FixtureCase, temp: &str) {
    let input = &case.input;
    let label = case.case.rsplit('/').next().unwrap_or_default();
    let work = Path::new(temp).join("skills-tools-cases").join(label);
    std::fs::create_dir_all(&work).expect("caso");
    let allow = input["allow_scripts"] == json!(true);
    let config = config(&work, allow);
    write_tree(&config.skills_dir);
    let library = library(&config);
    let hash = library.folder_hash("demo").expect("hash de la skill demo");
    let approvals_path = config.app_dir.join("skills_approved.json");
    if let Some(text) = approvals_text(input["approvals"].as_str().unwrap_or("missing"), &hash) {
        std::fs::create_dir_all(&config.app_dir).expect("app");
        std::fs::write(&approvals_path, text).expect("aprobaciones");
    }
    let runner = Arc::new(FakeRunner {
        calls: Mutex::new(Vec::new()),
    });
    let interpreter = input["interpreter"]
        .as_str()
        .map(|text| Some(PathBuf::from(text)));
    let fail_write = case.case.contains("approval-write-fails");
    let mut hooks = SkillHooks {
        interpreter: Some(interpreter.unwrap_or(None)),
        runner: Some(runner.clone()),
        write_approvals: None,
    };
    if fail_write {
        hooks.write_approvals = Some(Arc::new(|_: &Path, _: &Map<String, Value>| {
            Err(std::io::Error::other("disco lleno"))
        }));
    }
    let tools = build_skill_tools_with_hooks(&config, Arc::clone(&library), &hooks);
    let tools: std::collections::HashMap<String, ToolSpec> = tools
        .into_iter()
        .map(|tool| (tool.name.clone(), tool))
        .collect();
    let mut results = Vec::new();
    let mut infos = Vec::new();
    if let Some(calls) = input["calls"].as_array() {
        for call in calls {
            let name = call["tool"].as_str().expect("tool");
            let args = call["args"].as_object().cloned().unwrap_or_default();
            let tool = tools.get(name).expect("herramienta");
            results.push(call_result(tool, &args));
            infos.push(tool_info(&tool.describe(&args)));
        }
    }
    let final_text = std::fs::read(&approvals_path).ok().map(|bytes| {
        String::from_utf8_lossy(&bytes)
            .replace("\r\n", "\n")
            .replace('\r', "\n")
    });
    let calls = runner.calls.lock().expect("llamadas").clone();
    let value = normalize(
        &json!({
            "results": results,
            "infos": infos,
            "approvals": final_text,
            "approvals_exists": approvals_path.is_file(),
            "calls": calls,
        }),
        temp,
    );
    assert_eq!(value, case.output, "caso {}", case.case);
}

fn run_specs(case: &FixtureCase, temp: &str) {
    let work = Path::new(temp).join("skills-tools-cases").join("specs");
    std::fs::create_dir_all(&work).expect("specs");
    let config = config(&work, true);
    write_tree(&config.skills_dir);
    let library = library(&config);
    let tools = build_skill_tools_with_hooks(&config, library, &SkillHooks::default());
    let value = Value::Object(
        [(
            "tools".to_string(),
            Value::Array(
                tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                            "level": tool.level,
                            "untrusted_output": tool.untrusted_output,
                            "timeout_s": tool.timeout_s,
                            "schema": tool.schema(),
                            "info": tool_info(&tool.describe(&Map::new())),
                        })
                    })
                    .collect(),
            ),
        )]
        .into_iter()
        .collect(),
    );
    assert_eq!(normalize(&value, temp), case.output, "caso {}", case.case);
}

#[test]
fn every_tools_skills_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de tools_skills");
    let cases_in_file = cases.len();
    assert!(cases_in_file >= 68, "faltan casos: {cases_in_file}");
    let temp = std::env::temp_dir().join(format!("ez-tools-skills-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("temporal");
    let temp_text = temp.display().to_string();
    let mut run = 0;
    for case in &cases {
        match case.function.as_str() {
            "build_skill_tools" => {
                if case.case.ends_with("/specs") {
                    run_specs(case, &temp_text);
                } else {
                    run_call(case, &temp_text);
                }
            }
            "fixture_tree" => {
                // El árbol de SKILLS es el del oráculo; se comprueba al escribirlo.
                let names: Vec<&str> = SKILLS.iter().map(|(folder, _, _)| *folder).collect();
                assert_eq!(names.len(), 5);
            }
            other => panic!("función sin cubrir: {other}"),
        }
        run += 1;
    }
    assert_eq!(run, cases_in_file, "se saltó algún caso");
    let _ = std::fs::remove_dir_all(&temp);
    assert_eq!(SCRIPT_TIMEOUT_S, 30.0);
    assert_eq!(INTERPRETER, "C:/Python/python.exe");
}
