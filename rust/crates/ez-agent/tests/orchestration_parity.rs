//! Paridad de `agent/orchestration.py` contra
//! `parity/fixtures/orchestration.jsonl` (P-AGENT-A).

use std::path::Path;

use ez_agent::orchestration::{
    chain_spec, describe_steps, flow_task, load_guide, parse_steps, ChainOptions, Step,
    FLOW_PREFIX, MAX_STEPS, ORCHESTRATION_HINT,
};
use ez_core::pyvalue::python_str;
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/orchestration.jsonl";

fn field<'a>(input: &'a Value, name: &str) -> &'a Value {
    input.get(name).unwrap_or(&Value::Null)
}

fn steps_from(value: &Value) -> Vec<Step> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, item)| item.as_str().map(|text| (key.clone(), text.to_string())))
                .collect::<Step>()
        })
        .collect()
}

fn flow_names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(python_str)
        .collect()
}

fn chain_case(input: &Value) -> Value {
    let steps = parse_steps(field(input, "then")).expect("pasos válidos del caso");
    let flow = input.get("flow").map(flow_names);
    let options = ChainOptions {
        cwd: input.get("cwd").and_then(Value::as_str),
        space: input.get("space").and_then(Value::as_str),
        notify: field(input, "notify").as_bool().unwrap_or(false),
        deliver: field(input, "deliver").as_bool().unwrap_or(false),
        chain_id: field(input, "chain_id").as_str().unwrap_or_default(),
        flow: flow.as_deref(),
    };
    json!({
        "spec": chain_spec(field(input, "source").as_str().unwrap_or_default(), &steps, &options)
    })
}

fn step_value(step: &Step) -> Value {
    Value::Object(
        step.iter()
            .map(|(key, value)| (key.clone(), json!(value)))
            .collect(),
    )
}

fn run_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "parse_steps" => {
            let steps = parse_steps(field(input, "then"))?;
            let values: Vec<Value> = steps.iter().map(step_value).collect();
            json!({"steps": values})
        }
        "chain_spec" => chain_case(input),
        "describe_steps" => json!({"text": describe_steps(&steps_from(field(input, "steps")))}),
        "flow_task" => json!({"text": flow_task(&flow_names(field(input, "names")))}),
        "load_guide" => json!({
            "guide": load_guide(field(input, "agent_key").as_str().unwrap_or_default())
        }),
        "module_constants" => json!({
            "max_steps": MAX_STEPS,
            "flow_prefix": FLOW_PREFIX,
            "orchestration_hint": ORCHESTRATION_HINT,
        }),
        other => return Err(format!("función sin portar: {other}")),
    })
}

#[test]
fn every_orchestration_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de orchestration legibles");
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for case in &cases {
        let got = run_case(case);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => value == &case.output,
            (Err(message), Some(expected)) => message == expected,
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{}: {got:?} != {:?} / {:?}",
                case.case, case.output, case.error
            ));
        }
        cases_run += 1;
    }
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 46, "faltan casos en el fixture");
}
