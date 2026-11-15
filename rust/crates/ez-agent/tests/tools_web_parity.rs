//! Paridad de `tools_web` contra `parity/fixtures/tools_web.jsonl` (P-AGENT-C).
//!
//! Un solo `#[test]`: los casos comparten el directorio temporal y el orden de
//! los fixtures. La red no se toca: el transporte, el DNS y el reloj se
//! inyectan como en el oráculo de Python.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ez_agent::registry::ToolResult;
use ez_agent::search::{DdgsSearchProvider, SearchError, SearchProvider, SearchResult};
use ez_agent::tools_web::{
    build_web_tools, build_web_tools_with_hooks, extract_text, is_blocked_ip,
    pinned_request_target_json, run_fetch_page_json, run_web_search_json,
    validate_public_target_json, wrap_untrusted, FetchHooks, WebRequest, FETCH_TIMEOUT_S,
    MAX_FETCH_BYTES, MAX_QUERY_CHARS, MAX_REDIRECTS, MAX_SEARCH_RESULTS, USER_AGENT,
};
use ez_core::agent_config::AgentConfig;
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/tools_web.jsonl";
const PUBLIC: &str = "93.184.216.34";

#[path = "tools_web_parity/fakes.rs"]
mod fakes;
use fakes::{
    address, page, resolver_from, target_resolver, test_page, Dns, FakeClock, FakeResolver,
    FakeTransport,
};

/// Casos que prueban la forma interna del paquete `ddgs` o de `bs4`/`lxml`,
/// que el Rust no tiene: usa HTTP directo y un parser compilado.
const STRUCTURAL: [&str; 22] = [
    "tools_web/DdgsSearchProvider/bad-shape",
    "tools_web/DdgsSearchProvider/empty",
    "tools_web/DdgsSearchProvider/error/oserror",
    "tools_web/DdgsSearchProvider/error/provider",
    "tools_web/DdgsSearchProvider/error/rate-limit",
    "tools_web/DdgsSearchProvider/error/timeout",
    "tools_web/DdgsSearchProvider/kwargs",
    "tools_web/DdgsSearchProvider/limit",
    "tools_web/DdgsSearchProvider/load-ok",
    "tools_web/DdgsSearchProvider/missing",
    "tools_web/DdgsSearchProvider/new-engine",
    "tools_web/DdgsSearchProvider/none",
    "tools_web/DdgsSearchProvider/normal",
    "tools_web/DdgsSearchProvider/other-error",
    "tools_web/DdgsSearchProvider/results/no-results",
    "tools_web/DdgsSearchProvider/skip",
    "tools_web/DdgsSearchProvider/state/oserror",
    "tools_web/DdgsSearchProvider/state/provider",
    "tools_web/DdgsSearchProvider/state/rate-limit",
    "tools_web/DdgsSearchProvider/state/timeout",
    "tools_web/extract/no-bs4",
    "tools_web/extract/no-lxml",
];

fn config(max_fetch_chars: i64, block_private_ips: bool, dir: &std::path::Path) -> AgentConfig {
    let (mut config, _) = AgentConfig::load(Some(&dir.join("permissions.toml")));
    config.web.max_fetch_chars = max_fetch_chars;
    config.web.block_private_ips = block_private_ips;
    config
}

fn request_record(request: &WebRequest) -> Value {
    let header = |name: &str| {
        request
            .headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| Value::String(value.clone()))
            .unwrap_or(Value::Null)
    };
    json!({
        "method": request.method,
        "url": request.url,
        "host": header("host"),
        "user_agent": header("user-agent"),
        "accept": header("accept"),
        "sni": Value::Null,
    })
}

fn calls_json(calls: &[WebRequest]) -> Value {
    Value::Array(calls.iter().map(request_record).collect())
}

fn info_json(info: &ez_agent::registry::ToolInfo) -> Value {
    json!({
        "level": info.level,
        "bulk": info.bulk,
        "always_confirm": info.always_confirm,
        "summary": info.summary,
    })
}

fn pairs_json(pairs: &[(String, String)]) -> Value {
    Value::Object(
        pairs
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect::<Map<String, Value>>(),
    )
}

fn with_temp<T>(dir: &std::path::Path, run: impl FnOnce() -> T) -> T {
    std::fs::create_dir_all(dir).expect("directorio temporal");
    run()
}

#[test]
fn every_tools_web_case_matches_python() {
    let cases = fixtures::load(std::path::Path::new(FIXTURES)).expect("fixtures de tools_web");
    let cases_in_file = cases.len();
    assert!(cases_in_file >= 240, "faltan casos: {cases_in_file}");
    let temp = std::env::temp_dir().join(format!("ez-tools-web-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("temporal");
    let mut run = 0;
    let mut skipped = 0;
    for case in &cases {
        if STRUCTURAL.contains(&case.case.as_str()) {
            skipped += 1;
            continue;
        }
        run_case(case, &temp, &mut run);
    }
    assert_eq!(run + skipped, cases_in_file, "se saltó algún caso");
    assert_eq!(skipped, STRUCTURAL.len(), "faltan omisiones documentadas");
    let _ = std::fs::remove_dir_all(&temp);
}

fn run_case(case: &FixtureCase, temp: &std::path::Path, run: &mut usize) {
    let input = &case.input;
    match case.function.as_str() {
        "module_constants" => {
            assert_eq!(case.output["default_max_chars"], 8000);
            assert_eq!(case.output["max_search_results"], MAX_SEARCH_RESULTS);
            assert_eq!(case.output["max_query_chars"], MAX_QUERY_CHARS);
            assert_eq!(case.output["fetch_timeout_s"], FETCH_TIMEOUT_S);
            assert_eq!(case.output["max_redirects"], MAX_REDIRECTS);
            assert_eq!(case.output["max_fetch_bytes"], MAX_FETCH_BYTES);
            assert_eq!(case.output["user_agent"], USER_AGENT);
            assert_eq!(SearchError::new("x").kind, "provider");
        }
        "wrap_untrusted" => {
            let value = json!({"text": wrap_untrusted(
                input["source"].as_str().expect("source"),
                input["content"].as_str().expect("content"),
            )});
            assert_eq!(value, case.output, "caso {}", case.case);
        }
        "_is_blocked_ip" => {
            if let Some(list) = input.get("addresses").and_then(Value::as_array) {
                for (index, raw) in list.iter().enumerate() {
                    let expected = &case.output["cases"][index]["blocked"];
                    assert_eq!(
                        json!(is_blocked_ip(address(raw.as_str().expect("IP")))),
                        *expected,
                        "caso {} dirección {index}",
                        case.case
                    );
                }
            } else {
                let address = address(input["address"].as_str().expect("address"));
                assert_eq!(
                    json!({"blocked": is_blocked_ip(address)}),
                    case.output,
                    "caso {}",
                    case.case
                );
            }
        }
        "validate_public_target" | "pinned_request_target" => {
            let dir = temp.join(safe(&case.case));
            with_temp(&dir, || {
                let resolver = target_resolver();
                let config = config(8000, input["block_private_ips"] == json!(true), &dir);
                let url = &input["url"];
                if case.function == "validate_public_target" {
                    let outcome = validate_public_target_json(url, &config, &resolver);
                    compare_target(case, outcome);
                } else {
                    let outcome = pinned_request_target_json(url, &config, &resolver);
                    match outcome {
                        Ok((connect_url, headers, extensions)) => {
                            assert_eq!(case.error, None, "caso {}", case.case);
                            let value = json!({
                                "connect_url": connect_url,
                                "headers": pairs_json(&headers),
                                "extensions": pairs_json(&extensions),
                            });
                            assert_eq!(value, case.output, "caso {}", case.case);
                        }
                        Err(error) => assert_eq!(
                            Some(&error.message),
                            case.error.as_ref(),
                            "caso {}",
                            case.case
                        ),
                    }
                }
            });
        }
        "run_web_search" => {
            let provider = FakeProvider::for_case(&case.case, input.get("results"));
            let outcome = run_web_search_json(&provider, &input["query"], &input["max_results"]);
            let value = json!({"result": outcome.to_dict(), "calls": provider.calls_json()});
            assert_eq!(value, case.output, "caso {}", case.case);
        }
        "DdgsSearchProvider.search" => {
            let outcome =
                DdgsSearchProvider::new().search_json(&input["query"], &input["max_results"]);
            match outcome {
                Ok(results) => {
                    assert_eq!(case.error, None, "caso {}", case.case);
                    let value = json!({"results": results
                        .iter()
                        .map(|item| json!({"title": item.title, "url": item.url, "snippet": item.snippet}))
                        .collect::<Vec<_>>()});
                    assert_eq!(value, case.output, "caso {}", case.case);
                }
                Err(error) => assert_eq!(
                    Some(&error.message),
                    case.error.as_ref(),
                    "caso {}",
                    case.case
                ),
            }
        }
        "_extract_text" => {
            let text = extract_text(input["html"].as_str().expect("html"));
            assert_eq!(json!({"text": text}), case.output, "caso {}", case.case);
        }
        "run_fetch_page" => {
            let dir = temp.join(safe(&case.case));
            with_temp(&dir, || run_fetch_case(case, &dir));
        }
        "build_web_tools" => run_build_case(case, temp),
        other => panic!("función sin cubrir: {other}"),
    }
    *run += 1;
}

fn safe(case: &str) -> String {
    case.replace(['/', '\\', ':'], "-")
}

fn compare_target(
    case: &FixtureCase,
    outcome: Result<ez_agent::tools_web::PublicTarget, ez_agent::tools_web::WebToolError>,
) {
    match outcome {
        Ok(target) => {
            assert_eq!(case.error, None, "caso {}", case.case);
            let value = json!({
                "url": target.url,
                "host": target.host,
                "port": target.port,
                "addresses": target.addresses.iter().map(ToString::to_string).collect::<Vec<_>>(),
            });
            assert_eq!(value, case.output, "caso {}", case.case);
        }
        Err(error) => assert_eq!(
            Some(&error.message),
            case.error.as_ref(),
            "caso {}",
            case.case
        ),
    }
}

fn run_fetch_case(case: &FixtureCase, dir: &std::path::Path) {
    let input = &case.input;
    let label = case.case.rsplit('/').next().unwrap_or_default();
    let transport = Arc::new(FakeTransport {
        calls: Mutex::new(Vec::new()),
        handler: test_page(label),
    });
    let resolver = Arc::new(resolver_from(input));
    let clock = Arc::new(FakeClock::new(if label == "budget" {
        &[0.0, 9.0]
    } else {
        &[1000.0; 4]
    }));
    let hooks = FetchHooks {
        transport: Some(transport.clone()),
        resolver: Some(resolver),
        clock: Some(clock),
        pin_requests: false,
    };
    let config = config(8000, input["block_private_ips"] == json!(true), dir);
    let outcome = run_fetch_page_json(&config, &input["url"], &input["max_chars"], &hooks);
    let calls = transport.calls.lock().expect("llamadas").clone();
    let value = json!({"result": outcome.to_dict(), "requests": calls_json(&calls)});
    assert_eq!(value, case.output, "caso {}", case.case);
}

fn run_build_case(case: &FixtureCase, temp: &std::path::Path) {
    let dir = temp.join(safe(&case.case));
    with_temp(&dir, || match case.case.as_str() {
        "tools_web/build_web_tools/specs" => {
            let config = config(8000, true, &dir);
            let tools = build_web_tools(&config, Some(Arc::new(FakeProvider::empty())));
            let value = json!({"tools": tools.iter().map(tool_json).collect::<Vec<_>>()});
            assert_eq!(value, case.output, "caso {}", case.case);
        }
        "tools_web/build_web_tools/describe" => {
            let config = config(8000, true, &dir);
            let tools = build_web_tools(&config, Some(Arc::new(FakeProvider::empty())));
            let value = Value::Array(
                case.output["cases"]
                    .as_array()
                    .expect("casos")
                    .iter()
                    .map(|item| {
                        let args = item["args"].as_object().cloned().unwrap_or_default();
                        json!({
                            "args": item["args"],
                            "search": info_json(&tools[0].describe(&args)),
                            "fetch": info_json(&tools[1].describe(&args)),
                        })
                    })
                    .collect(),
            );
            assert_eq!(value, case.output["cases"], "caso {}", case.case);
        }
        "tools_web/build_web_tools/web_search" => {
            let config = config(8000, true, &dir);
            let provider = Arc::new(FakeProvider::from(Some(&json!([
                {"title": "Uno", "url": "https://uno.test", "snippet": "primero"},
                {"title": "Dos", "url": "https://dos.test", "snippet": "segundo"},
            ]))));
            let tools = build_web_tools(&config, Some(provider.clone()));
            let mut results = Vec::new();
            let mut infos = Vec::new();
            let mut calls = Vec::new();
            for args in [
                json!({"query": "consulta"}),
                json!({"query": "  consulta  ", "max_results": 1}),
                json!({"query": ""}),
                json!({"query": "x", "max_results": 20}),
                json!({"query": "x", "max_results": true}),
                json!({"max_results": 3}),
            ] {
                let map = args.as_object().cloned().unwrap_or_default();
                results.push(run_tool(&tools[0], &map));
                infos.push(info_json(&tools[0].describe(&map)));
                calls.push(provider.last_call());
            }
            let value = json!({"results": results, "infos": infos, "calls": calls});
            assert_eq!(value, case.output, "caso {}", case.case);
        }
        "tools_web/build_web_tools/fetch_page" => {
            let transport = Arc::new(FakeTransport {
                calls: Mutex::new(Vec::new()),
                handler: Box::new(move |_| Ok(page(200, &[b'z'; 200], "text/html; charset=utf-8"))),
            });
            let mut mapping = BTreeMap::new();
            mapping.insert(
                "example.test".to_string(),
                Dns::Addresses(vec![address(PUBLIC)]),
            );
            let hooks = FetchHooks {
                transport: Some(transport.clone()),
                resolver: Some(Arc::new(FakeResolver(mapping))),
                clock: None,
                pin_requests: true,
            };
            let config = config(120, true, &dir);
            let tools =
                build_web_tools_with_hooks(&config, Some(Arc::new(FakeProvider::empty())), &hooks);
            let mut results = Vec::new();
            let mut infos = Vec::new();
            for args in [
                json!({"url": "http://example.test/x"}),
                json!({"url": "http://example.test/x", "max_chars": 10}),
                json!({"url": "http://example.test/x", "max_chars": 121}),
                json!({"url": "http://example.test/x", "max_chars": 0}),
                json!({"url": "http://127.0.0.1/x"}),
                json!({"url": ""}),
            ] {
                let map = args.as_object().cloned().unwrap_or_default();
                results.push(run_tool(&tools[1], &map));
                infos.push(info_json(&tools[1].describe(&map)));
            }
            let calls = transport.calls.lock().expect("llamadas").clone();
            let urls: Vec<Value> = calls.iter().map(|item| json!(item.url)).collect();
            let value = json!({"results": results, "infos": infos, "requests": urls});
            assert_eq!(value, case.output, "caso {}", case.case);
        }
        other => panic!("caso build sin cubrir: {other}"),
    });
}

fn tool_json(tool: &ez_agent::registry::ToolSpec) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
        "level": tool.level,
        "untrusted_output": tool.untrusted_output,
        "timeout_s": tool.timeout_s,
        "schema": tool.schema(),
        "info": info_json(&tool.describe(&Map::new())),
    })
}

/// Proveedor falso: devuelve los datos crudos del fixture.
struct FakeProvider {
    raw: Value,
    error: Option<SearchError>,
    calls: Mutex<Vec<Value>>,
}

impl FakeProvider {
    fn empty() -> Self {
        Self {
            raw: json!([]),
            error: None,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn from(raw: Option<&Value>) -> Self {
        Self {
            raw: raw.cloned().unwrap_or_else(|| json!([])),
            error: None,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn for_case(case: &str, raw: Option<&Value>) -> Self {
        let raw = raw.cloned().unwrap_or_else(|| json!([]));
        let error = match case.rsplit('/').next().unwrap_or_default() {
            "search-error" => Some(SearchError::new("proveedor caído")),
            "search-timeout" => Some(SearchError::with_kind("tardó", "timeout")),
            "search-unavailable" => Some(SearchError::unavailable("falta")),
            "generic" => Some(SearchError::new("La búsqueda web falló: boom")),
            _ => None,
        };
        Self {
            raw,
            error,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls_json(&self) -> Value {
        Value::Array(self.calls.lock().expect("llamadas").clone())
    }

    fn last_call(&self) -> Value {
        self.calls
            .lock()
            .expect("llamadas")
            .last()
            .cloned()
            .unwrap_or(Value::Null)
    }
}

impl SearchProvider for FakeProvider {
    fn search(&self, _query: &str, _max_results: i64) -> Result<Vec<SearchResult>, SearchError> {
        Ok(Vec::new())
    }

    fn search_raw(&self, query: &str, max_results: i64) -> Result<Value, SearchError> {
        self.calls
            .lock()
            .expect("llamadas")
            .push(json!({"query": query, "max_results": max_results}));
        match &self.error {
            Some(error) => Err(error.clone()),
            None => Ok(self.raw.clone()),
        }
    }
}

fn run_tool(tool: &ez_agent::registry::ToolSpec, args: &Map<String, Value>) -> Value {
    (tool.handler)(args)
        .map(|value| match value {
            ez_agent::registry::ToolReturn::Result(result) => result.to_dict(),
            ez_agent::registry::ToolReturn::Value(value) => value,
        })
        .unwrap_or_else(|error| ToolResult::failed(error).to_dict())
}
