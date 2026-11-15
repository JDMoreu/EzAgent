//! Port of `SSE parsing in llm.py` (F4-06).
//!
//! Lector de Server-Sent Events sobre cualquier `Read`: acumula las líneas
//! `data:` de un evento hasta la línea en blanco y entrega el bloque ya unido
//! con `\n`, igual que `iter_sse` del Python. Las líneas de otros campos
//! (`event:`, `id:`) y los comentarios (`:`) no llevan datos y se ignoran.

use std::io::{BufRead, BufReader, Read};

use ez_core::pyvalue::py_strip;

/// Lector SSE incremental: un `next` por evento, sin cargar la respuesta entera.
pub struct SseReader<R: BufRead> {
    reader: R,
    lines: Vec<String>,
    done: bool,
    /// Entregar el bloque pendiente al cerrarse el flujo (`iter_sse` del Python).
    flush_last: bool,
}

impl<R: BufRead> SseReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            lines: Vec::new(),
            done: false,
            flush_last: true,
        }
    }

    /// Como `new`, pero descarta el bloque sin línea en blanco al final (el
    /// decodificador SSE del SDK de OpenAI no lo entrega).
    pub fn new_strict(reader: R) -> Self {
        Self {
            flush_last: false,
            ..Self::new(reader)
        }
    }

    /// Siguiente bloque `data:` (sus líneas unidas con `\n`); `None` al terminar.
    ///
    /// Un bloque termina en la línea en blanco o al cerrarse el flujo, como en
    /// el `iter_sse` del Python. Los errores de lectura se entregan una vez.
    pub fn next_payload(&mut self) -> Option<std::io::Result<String>> {
        loop {
            if self.done && self.lines.is_empty() {
                return None;
            }
            let mut raw = Vec::new();
            match self.reader.read_until(b'\n', &mut raw) {
                Ok(0) => {
                    self.done = true;
                    if self.lines.is_empty() {
                        return None;
                    }
                    if !self.flush_last {
                        self.lines.clear();
                        return None;
                    }
                    return Some(Ok(std::mem::take(&mut self.lines).join("\n")));
                }
                Ok(_) => {
                    // DECISION: httpx decodifica con `errors="replace"`; una
                    // línea con bytes inválidos no debe cortar el turno.
                    let text = String::from_utf8_lossy(&raw);
                    let line = py_strip(text.as_ref());
                    if line.is_empty() {
                        if !self.lines.is_empty() {
                            return Some(Ok(std::mem::take(&mut self.lines).join("\n")));
                        }
                    } else if let Some(data) = line.strip_prefix("data:") {
                        self.lines.push(py_strip(data).to_string());
                    }
                }
                Err(error) => {
                    self.done = true;
                    self.lines.clear();
                    return Some(Err(error));
                }
            }
        }
    }
}

impl<R: Read> SseReader<BufReader<R>> {
    /// Envuelve un `Read` (por ejemplo, la respuesta HTTP) en un lector SSE.
    pub fn over(reader: R) -> Self {
        Self::new(BufReader::new(reader))
    }
}

impl<R: BufRead> Iterator for SseReader<R> {
    type Item = std::io::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_payload()
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread::{self, JoinHandle};

    /// Servidor HTTP de una sola respuesta, en un hilo, para las pruebas.
    pub struct FakeServer {
        pub base_url: String,
        thread: Option<JoinHandle<()>>,
    }

    impl FakeServer {
        pub fn serve(status: &str, headers: &[(&str, &str)], body: &str) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("puerto de prueba libre");
            let address = listener
                .local_addr()
                .expect("dirección del servidor de prueba");
            let response = build_response(status, headers, body);
            let thread = thread::spawn(move || {
                if let Ok((mut socket, _)) = listener.accept() {
                    read_request(&mut socket);
                    let _ = socket.write_all(response.as_bytes());
                    let _ = socket.flush();
                }
            });
            Self {
                base_url: format!("http://{address}"),
                thread: Some(thread),
            }
        }
    }

    impl Drop for FakeServer {
        fn drop(&mut self) {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn build_response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }
        response.push_str("\r\n");
        response.push_str(body);
        response
    }

    fn read_request(socket: &mut TcpStream) {
        let mut data = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            match socket.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(read) => data.extend_from_slice(&buffer[..read]),
            }
            let Some(head) = find_headers_end(&data) else {
                continue;
            };
            let length = content_length(&data[..head]);
            if data.len() >= head + length {
                return;
            }
        }
    }

    fn find_headers_end(data: &[u8]) -> Option<usize> {
        data.windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
    }

    fn content_length(head: &[u8]) -> usize {
        let text = String::from_utf8_lossy(head);
        text.lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn payloads(text: &str) -> Vec<String> {
        SseReader::new(Cursor::new(text.as_bytes()))
            .filter_map(Result::ok)
            .collect()
    }

    #[test]
    fn reads_data_events_and_ignores_other_fields() {
        let text = ": comentario\nevent: message\ndata: uno\n\ndata: dos\n\n";
        assert_eq!(payloads(text), ["uno", "dos"]);
    }

    #[test]
    fn joins_multiline_data_with_newlines() {
        let text = "data: {\"a\": 1,\ndata: \"b\": 2}\n\n";
        assert_eq!(payloads(text), ["{\"a\": 1,\n\"b\": 2}"]);
    }

    #[test]
    fn flushes_the_last_block_at_eof() {
        assert_eq!(payloads("data: final"), ["final"]);
        assert_eq!(payloads("data: final\n"), ["final"]);
    }

    #[test]
    fn handles_crlf_and_data_without_space() {
        assert_eq!(
            payloads("data:uno\r\n\r\ndata: dos\r\n\r\n"),
            ["uno", "dos"]
        );
    }

    #[test]
    fn read_errors_are_reported_once() {
        let mut reader = SseReader::new(Cursor::new(b"data: uno\n\n".as_slice()));
        assert_eq!(
            reader.next_payload().and_then(Result::ok).as_deref(),
            Some("uno")
        );
        assert!(reader.next_payload().is_none());
    }
}
