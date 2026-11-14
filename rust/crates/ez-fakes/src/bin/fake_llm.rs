//! Binario `fake-llm`: servidor local de Chat Completions y Messages (F1-11).

use std::process::ExitCode;

const USAGE: &str = "\
fake-llm: imita OpenAI Chat Completions y Anthropic Messages con SSE.

Uso:
  fake-llm --scenario RUTA [--record RUTA] [--port N]

Opciones:
  --scenario RUTA  Escenario JSON (respuestas por turno, errores y cortes).
  --record RUTA    Archivo JSONL donde anotar cada petición (defecto: requests.jsonl).
  --port N         Puerto en 127.0.0.1 (defecto: 8790).

La app Python se apunta sin tocar src/:
  OpenAI:    LLM_PROVIDER=custom LLM_API_STYLE=openai
             LLM_BASE_URL=http://127.0.0.1:8790/v1 LLM_MODEL=fake-model
             LLM_API_KEY=sin-clave
  Anthropic: LLM_PROVIDER=custom LLM_API_STYLE=anthropic
             LLM_BASE_URL=http://127.0.0.1:8790 LLM_MODEL=fake-model
             LLM_API_KEY=sin-clave
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match ez_fakes::fake_llm::Options::parse(&args)
        .and_then(|options| ez_fakes::fake_llm::run(&options))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fake-llm: {error}");
            ExitCode::FAILURE
        }
    }
}
