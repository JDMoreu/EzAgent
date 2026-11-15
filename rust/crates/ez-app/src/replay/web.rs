//! Servidor HTTP local para el escenario de taint (F7-01b).
//!
//! Porta `LocalPage` de `parity/replay/run_py.py`: escucha en `127.0.0.1` con
//! un puerto del sistema y devuelve el HTML del escenario. Nunca red real.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Página servida en un puerto efímero de `127.0.0.1`.
pub struct LocalPage {
    url: String,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl LocalPage {
    /// Levanta el servidor y devuelve la URL base (`http://127.0.0.1:<puerto>`).
    pub fn start(body: &str) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("no pude abrir la página local: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("no pude configurar la página local: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("sin dirección local: {error}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let payload = body.as_bytes().to_vec();
        let handle = std::thread::Builder::new()
            .name("services".to_string())
            .spawn(move || serve(&listener, &payload, &flag))
            .map_err(|error| format!("no pude crear el hilo de la página local: {error}"))?;
        Ok(Self {
            url: format!("http://{address}"),
            stop,
            handle: Some(handle),
        })
    }

    /// URL que sustituye a `{web_url}` en el YAML.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// `stop`: apaga el hilo y cierra el socket.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for LocalPage {
    fn drop(&mut self) {
        self.stop();
    }
}

fn serve(listener: &TcpListener, body: &[u8], stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = answer(stream, body);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Una petición: se lee la cabecera y se responde como `BaseHTTPRequestHandler`.
fn answer(mut stream: TcpStream, body: &[u8]) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while request.len() < 8 * 1024 {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let header = format!(
        "HTTP/1.0 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_serves_its_body_on_a_local_port() {
        let mut page = LocalPage::start("<html>Hola</html>").expect("página");
        assert!(page.url().starts_with("http://127.0.0.1:"));
        let address = page.url().trim_start_matches("http://").to_string();
        let mut stream = TcpStream::connect(address).expect("conexión");
        stream
            .write_all(b"GET /pagina HTTP/1.1\r\nHost: x\r\n\r\n")
            .expect("petición");
        let mut text = String::new();
        stream.read_to_string(&mut text).expect("respuesta");
        assert!(text.contains("200 OK"));
        assert!(text.ends_with("<html>Hola</html>"));
        page.stop();
    }
}
