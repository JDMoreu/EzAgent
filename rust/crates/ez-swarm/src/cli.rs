//! Port of `src/ezagent/swarm/cli.py` (F3-13).
//!
//! `ezs`: the command with which an agent talks to its swarm. It is
//! deliberately minimal and only uses the standard library so it starts fast:
//! it sends its arguments to the local EzAgent service and prints the reply.
//! Which agent is speaking is stated by its token (`EZS_TOKEN`), which EzAgent
//! puts in the environment of the terminal it launches.
//!
//! Exit codes: 0 fine, 1 the board rejected the command, 2 no connection or no
//! token.

use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use ez_core::pyjson::dumps;
use ez_core::pyloads;
use ez_core::pyvalue::{head_chars, is_py_space, py_strip, python_str, truthy};
use serde_json::{json, Value};

/// Attempts before giving up when EzAgent does not answer.
pub const RETRIES: usize = 3;
/// Timeout of a normal command.
pub const SHORT_TIMEOUT: f64 = 10.0;
/// Timeout of `wait`: above the server's maximum wait (110 s).
pub const WAIT_TIMEOUT: f64 = 130.0;

const LOCAL_PREFIX: &str = "http://127.0.0.1:";
const NO_SEAT: &str = "Esta terminal no pertenece a un enjambre de EzAgent (falta EZS_SEAT). No lo investigues: termina tu turno.";
const TOO_SLOW: &str = "El tablero tardó demasiado. Comprueba con: ezs board";
const NO_RESPONSE: &str =
    "EzAgent no responde. No lo investigues: sigue con tu tarea y reintenta en un minuto.";

/// Why a request did not produce a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostError {
    /// The server took longer than the timeout.
    Timeout,
    /// The connection failed or the answer was not valid JSON.
    Failed,
}

/// `endpoint_path`: file where EzAgent publishes its address.
pub fn endpoint_path() -> String {
    endpoint_path_from(&|name| std::env::var(name).ok())
}

/// [`endpoint_path`] with an injectable environment.
pub fn endpoint_path_from(get: &dyn Fn(&str) -> Option<String>) -> String {
    if let Some(custom) = get("EZS_ENDPOINT").filter(|value| !value.is_empty()) {
        return custom;
    }
    let seat = get("EZS_SEAT").map_or_else(String::new, |value| py_strip(&value).to_string());
    if !seat.is_empty() {
        // <base>/seats/<asiento> → <base>/endpoint.json
        let absolute = absolute_path(&seat);
        let first = absolute.parent().unwrap_or(&absolute);
        let base = first.parent().unwrap_or(first);
        return base.join("endpoint.json").to_string_lossy().into_owned();
    }
    let base = get("APPDATA")
        .filter(|value| !value.is_empty())
        .or_else(|| get("XDG_CONFIG_HOME").filter(|value| !value.is_empty()));
    let root = base.map_or_else(|| ez_core::dirs::home().join(".config"), PathBuf::from);
    root.join("EzAgent")
        .join("swarm")
        .join("endpoint.json")
        .to_string_lossy()
        .into_owned()
}

fn absolute_path(path: &str) -> PathBuf {
    // `std::path::absolute` resolves `.`/`..` lexically without touching the
    // filesystem, like `os.path.abspath`.
    std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path))
}

/// `_read_port`: local port published by EzAgent, or 0 if it is not listening.
pub fn read_port() -> u16 {
    read_port_from(&|name| std::env::var(name).ok())
}

/// [`read_port`] with an injectable environment.
pub fn read_port_from(get: &dyn Fn(&str) -> Option<String>) -> u16 {
    let Ok(text) = fs::read_to_string(endpoint_path_from(get)) else {
        return 0;
    };
    let Ok(Value::Object(data)) = pyloads::loads(&text) else {
        return 0;
    };
    let url = data
        .get("url")
        .filter(|value| truthy(value))
        .map_or_else(String::new, python_str);
    let Some(rest) = url.strip_prefix(LOCAL_PREFIX) else {
        return 0;
    };
    let port = rest.split('/').next().unwrap_or("");
    // DECISION: Python's `str.isdigit()` also accepts Unicode digits; the port
    // is always ASCII in `endpoint.json`, so only ASCII digits are accepted.
    if port.is_empty() || !port.chars().all(|character| character.is_ascii_digit()) {
        return 0;
    }
    port.parse().unwrap_or(0)
}

/// `_token`: member token from `EZS_TOKEN` or the file `EZS_SEAT` points to.
pub fn token() -> String {
    token_from(&|name| std::env::var(name).ok())
}

/// [`token`] with an injectable environment.
pub fn token_from(get: &dyn Fn(&str) -> Option<String>) -> String {
    let token = get("EZS_TOKEN").map_or_else(String::new, |value| py_strip(&value).to_string());
    let seat = get("EZS_SEAT").map_or_else(String::new, |value| py_strip(&value).to_string());
    if token.is_empty() && !seat.is_empty() {
        return match fs::read_to_string(&seat) {
            Ok(text) => py_strip(head_chars(&text, 200)).to_string(),
            // DECISION: an unreadable or non-UTF-8 seat file counts as no token
            // instead of panicking like Python's UnicodeDecodeError would.
            Err(_) => String::new(),
        };
    }
    token
}

/// One hand-written HTTP/1.0 request: avoids loading a full client.
pub fn post(
    port: u16,
    token: &str,
    argv: &[String],
    timeout: f64,
) -> Result<(bool, String), PostError> {
    let body = dumps(&json!({"argv": argv}));
    let request = format!(
        "POST /v1 HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let mut message = request.into_bytes();
    message.extend_from_slice(body.as_bytes());
    let duration = Duration::from_secs_f64(timeout);
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, duration).map_err(classify)?;
    stream.set_read_timeout(Some(duration)).map_err(classify)?;
    stream.set_write_timeout(Some(duration)).map_err(classify)?;
    stream.write_all(&message).map_err(classify)?;
    let mut response = Vec::new();
    let mut buffer = [0u8; 65_536];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => response.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(classify(error)),
        }
    }
    let text =
        String::from_utf8(split_payload(&response).to_vec()).map_err(|_| PostError::Failed)?;
    let Value::Object(data) = pyloads::loads(&text).map_err(|_| PostError::Failed)? else {
        return Err(PostError::Failed);
    };
    let ok = data.get("ok").is_some_and(truthy);
    let reply = data
        .get("text")
        .filter(|value| truthy(value))
        .map_or_else(String::new, python_str);
    Ok((ok, reply))
}

fn split_payload(response: &[u8]) -> &[u8] {
    const SEPARATOR: &[u8] = b"\r\n\r\n";
    response
        .windows(SEPARATOR.len())
        .position(|window| window == SEPARATOR)
        .map_or(&[], |index| &response[index + SEPARATOR.len()..])
}

fn classify(error: io::Error) -> PostError {
    match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => PostError::Timeout,
        _ => PostError::Failed,
    }
}

fn out(stream: &mut dyn Write, text: &str) {
    let _ = writeln!(stream, "{}", text.trim_end_matches('\n'));
    let _ = stream.flush();
}

/// Entry point of `ezs`; `None` means `sys.argv[1:]`.
pub fn main(argv: Option<&[String]>) -> i32 {
    let args = match argv {
        Some(argv) => argv.to_vec(),
        None => std::env::args().skip(1).collect(),
    };
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let get = |name: &str| std::env::var(name).ok();
    main_with(&args, &get, &mut stdout, &mut stderr)
}

/// [`main`] with an injectable environment and streams.
pub fn main_with(
    argv: &[String],
    get: &dyn Fn(&str) -> Option<String>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let token = token_from(get);
    if token.is_empty() || !token.is_ascii() || token.chars().any(is_py_space) {
        out(stderr, NO_SEAT);
        return 2;
    }
    let timeout = if argv.first().map(String::as_str) == Some("wait") {
        WAIT_TIMEOUT
    } else {
        SHORT_TIMEOUT
    };
    for attempt in 0..RETRIES {
        let port = read_port_from(get);
        let result = if port == 0 {
            Err(PostError::Failed)
        } else {
            post(port, &token, argv, timeout)
        };
        match result {
            Ok((ok, text)) => {
                if ok {
                    out(stdout, &text);
                } else {
                    out(stderr, &text);
                }
                return if ok { 0 } else { 1 };
            }
            Err(PostError::Timeout) => {
                out(stderr, TOO_SLOW);
                return 2;
            }
            Err(PostError::Failed) => {
                if attempt + 1 < RETRIES {
                    thread::sleep(Duration::from_secs_f64(0.4 * (attempt + 1) as f64));
                }
            }
        }
    }
    out(stderr, NO_RESPONSE);
    2
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::TcpListener;
    use std::path::Path;

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("ez-swarm-cli-{}", uuid::Uuid::new_v4().simple()));
            fs::create_dir_all(&path).expect("temporary folder");
            Self(path)
        }

        fn path(&self) -> PathBuf {
            self.0.clone()
        }

        fn write(&self, name: &str, content: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, content).expect("temporary file");
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.0.join("endpoint.json"));
            let _ = fs::remove_file(self.0.join("token"));
            let _ = fs::remove_dir(&self.0);
        }
    }

    fn environment(pairs: &[(&str, Option<&str>)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, Option<String>> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), value.map(str::to_string)))
            .collect();
        move |name| map.get(name).cloned().flatten()
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("UTF-8 output")
    }

    #[test]
    fn endpoint_path_follows_the_environment() {
        let direct = environment(&[("EZS_ENDPOINT", Some("http://mi-host/endpoint.json"))]);
        assert_eq!(endpoint_path_from(&direct), "http://mi-host/endpoint.json");

        let folder = TempDir::new();
        let seat = folder
            .path()
            .join("swarm")
            .join("seats")
            .join("ana")
            .join("token");
        let seat_text = seat.to_string_lossy().into_owned();
        let seat_env = environment(&[("EZS_SEAT", Some(&seat_text))]);
        assert_eq!(
            Path::new(&endpoint_path_from(&seat_env)),
            folder
                .path()
                .join("swarm")
                .join("seats")
                .join("endpoint.json")
        );

        let profile = folder.path().join("profile");
        let profile_text = profile.to_string_lossy().into_owned();
        let appdata = environment(&[("APPDATA", Some(&profile_text))]);
        assert_eq!(
            Path::new(&endpoint_path_from(&appdata)),
            profile.join("EzAgent").join("swarm").join("endpoint.json")
        );

        let config = folder.path().join("config");
        let config_text = config.to_string_lossy().into_owned();
        let xdg = environment(&[("APPDATA", None), ("XDG_CONFIG_HOME", Some(&config_text))]);
        assert_eq!(
            Path::new(&endpoint_path_from(&xdg)),
            config.join("EzAgent").join("swarm").join("endpoint.json")
        );

        let empty = environment(&[("EZS_SEAT", Some("  "))]);
        assert!(Path::new(&endpoint_path_from(&empty)).ends_with(
            Path::new(".config")
                .join("EzAgent")
                .join("swarm")
                .join("endpoint.json")
        ));
    }

    #[test]
    fn read_port_reads_the_published_url() {
        let cases: [(&str, Option<&str>, u16); 9] = [
            ("valido", Some(r#"{"url": "http://127.0.0.1:8765"}"#), 8765),
            (
                "con-ruta",
                Some(r#"{"url": "http://127.0.0.1:8765/v1"}"#),
                8765,
            ),
            ("https", Some(r#"{"url": "https://127.0.0.1:8765"}"#), 0),
            (
                "puerto-texto",
                Some(r#"{"url": "http://127.0.0.1:abc"}"#),
                0,
            ),
            ("json-roto", Some("{no es json"), 0),
            ("sin-url", Some(r#"{"puerto": 1}"#), 0),
            ("url-vacia", Some(r#"{"url": ""}"#), 0),
            ("lista", Some("[]"), 0),
            ("sin-archivo", None, 0),
        ];
        for (id, content, expected) in cases {
            let folder = TempDir::new();
            let endpoint = folder.path().join("endpoint.json");
            if let Some(content) = content {
                fs::write(&endpoint, content).expect("endpoint file");
            }
            let endpoint_text = endpoint.to_string_lossy().into_owned();
            let env = environment(&[("EZS_ENDPOINT", Some(&endpoint_text))]);
            assert_eq!(read_port_from(&env), expected, "{id}");
        }
    }

    #[test]
    fn token_comes_from_the_environment_or_the_seat_file() {
        let from_env = environment(&[("EZS_TOKEN", Some("  abc  "))]);
        assert_eq!(token_from(&from_env), "abc");

        let folder = TempDir::new();
        let seat = folder.write("token", b"  token-123 \n");
        let seat_text = seat.to_string_lossy().into_owned();
        let from_seat = environment(&[("EZS_TOKEN", Some("")), ("EZS_SEAT", Some(&seat_text))]);
        assert_eq!(token_from(&from_seat), "token-123");

        let both = environment(&[("EZS_TOKEN", Some("env")), ("EZS_SEAT", Some(&seat_text))]);
        assert_eq!(token_from(&both), "env");

        let missing = folder.path().join("nada");
        let missing_text = missing.to_string_lossy().into_owned();
        let no_file = environment(&[("EZS_SEAT", Some(&missing_text))]);
        assert_eq!(token_from(&no_file), "");

        let long = folder.write("token", format!("{}y", "x".repeat(200)).as_bytes());
        let long_text = long.to_string_lossy().into_owned();
        let long_env = environment(&[("EZS_SEAT", Some(&long_text))]);
        assert_eq!(token_from(&long_env), "x".repeat(200));
    }

    #[test]
    fn main_rejects_tokens_without_a_seat() {
        let cases = [
            environment(&[("EZS_TOKEN", None)]),
            environment(&[("EZS_TOKEN", Some("a b"))]),
            environment(&[("EZS_TOKEN", Some("tökén"))]),
        ];
        for env in cases {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = main_with(&["board".to_string()], &env, &mut stdout, &mut stderr);
            assert_eq!(code, 2);
            assert!(stdout.is_empty());
            assert_eq!(text(&stderr), format!("{NO_SEAT}\n"));
        }
    }

    #[test]
    fn main_reports_a_missing_server() {
        let folder = TempDir::new();
        let endpoint = folder.path().join("no-existe.json");
        let endpoint_text = endpoint.to_string_lossy().into_owned();
        let env = environment(&[
            ("EZS_ENDPOINT", Some(&endpoint_text)),
            ("EZS_TOKEN", Some("abc")),
        ]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = main_with(&["board".to_string()], &env, &mut stdout, &mut stderr);
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert_eq!(text(&stderr), format!("{NO_RESPONSE}\n"));
    }

    fn fake_server(response: Vec<u8>) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("fake listener");
        let port = listener.local_addr().expect("fake port").port();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut received = Vec::new();
                let mut buffer = [0u8; 1024];
                while !received.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => received.extend_from_slice(&buffer[..count]),
                    }
                }
                let _ = stream.write_all(&response);
            }
        });
        port
    }

    fn reply(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes()
    }

    #[test]
    fn post_parses_the_server_reply() {
        let port = fake_server(reply(r#"{"ok": true, "text": "hola"}"#));
        assert_eq!(
            post(port, "tok", &["board".to_string()], 5.0),
            Ok((true, "hola".to_string()))
        );

        let port = fake_server(reply(r#"{"ok": false, "text": ""}"#));
        assert_eq!(
            post(port, "tok", &["board".to_string()], 5.0),
            Ok((false, String::new()))
        );

        let port = fake_server(b"sin cabeceras".to_vec());
        assert_eq!(
            post(port, "tok", &["board".to_string()], 5.0),
            Err(PostError::Failed)
        );
    }

    #[test]
    fn main_routes_the_reply_by_its_result() {
        let folder = TempDir::new();
        let port = fake_server(reply(r#"{"ok": true, "text": "hecho\n"}"#));
        let endpoint = folder.path().join("endpoint.json");
        fs::write(
            &endpoint,
            format!(r#"{{"url": "http://127.0.0.1:{port}/v1"}}"#),
        )
        .expect("endpoint file");
        let endpoint_text = endpoint.to_string_lossy().into_owned();
        let env = environment(&[
            ("EZS_ENDPOINT", Some(&endpoint_text)),
            ("EZS_TOKEN", Some("abc")),
        ]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = main_with(&["board".to_string()], &env, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(text(&stdout), "hecho\n");
        assert!(stderr.is_empty());

        let port = fake_server(reply(r#"{"ok": false, "text": "no puedes"}"#));
        fs::write(
            &endpoint,
            format!(r#"{{"url": "http://127.0.0.1:{port}/v1"}}"#),
        )
        .expect("endpoint file");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = main_with(&["board".to_string()], &env, &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert_eq!(text(&stderr), "no puedes\n");
    }
}
