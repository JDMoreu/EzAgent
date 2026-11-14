//! Port of `src/ezagent/voice/app.py` (`_Sentences`) (F3-16).
//!
//! Acumula los tokens del LLM y los corta en frases para el TTS. La primera
//! frase se emite en cuanto hay una cláusula corta (coma, punto y coma o dos
//! puntos) o un espacio antes de `first_chars`, para que el habla empiece antes
//! de que el modelo termine. Los cortes cuentan puntos de código, como `len()`
//! de Python.

use ez_core::pyvalue::{char_len, is_py_space, py_lstrip, py_strip};

pub const DEFAULT_MAX_CHARS: usize = 160;
pub const DEFAULT_FIRST_CHARS: usize = 40;

/// Acumulador de tokens en frases.
#[derive(Debug, Clone, PartialEq)]
pub struct Sentences {
    pub buf: String,
    pub max_chars: usize,
    pub first_chars: usize,
    first: bool,
}

impl Default for Sentences {
    // DECISION: Python tiene argumentos por defecto (`max_chars=160`,
    // `first_chars=40`); en Rust `new` pide ambos y `Default` da los de fábrica.
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CHARS, DEFAULT_FIRST_CHARS)
    }
}

/// Índice de byte del punto de código `index` (o el final del texto).
fn byte_at(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// `text[:index]` contando puntos de código.
fn head(text: &str, index: usize) -> &str {
    &text[..byte_at(text, index)]
}

/// `text[index:]` contando puntos de código.
fn tail(text: &str, index: usize) -> &str {
    &text[byte_at(text, index)..]
}

/// `re.search(r"[,;:](?=\s)", text)`: índice del signo, no del espacio.
fn first_clause(text: &str) -> Option<usize> {
    let mut chars = text.chars().enumerate().peekable();
    while let Some((index, character)) = chars.next() {
        if matches!(character, ',' | ';' | ':')
            && chars.peek().is_some_and(|(_, next)| is_py_space(*next))
        {
            return Some(index);
        }
    }
    None
}

/// `re.search(r"[.!?](?=\s|$)", text)`: índice después del signo.
fn sentence_end(text: &str) -> Option<usize> {
    let mut chars = text.chars().enumerate().peekable();
    while let Some((index, character)) = chars.next() {
        if matches!(character, '.' | '!' | '?')
            && chars.peek().is_none_or(|(_, next)| is_py_space(*next))
        {
            return Some(index + 1);
        }
    }
    None
}

/// `text.rfind(" ", 0, limit)` contando puntos de código.
fn rfind_space(text: &str, limit: usize) -> Option<usize> {
    let prefix = head(text, limit);
    let byte = prefix.rfind(' ')?;
    Some(prefix[..byte].chars().count())
}

impl Sentences {
    pub fn new(max_chars: usize, first_chars: usize) -> Self {
        Self {
            buf: String::new(),
            max_chars,
            first_chars,
            first: true,
        }
    }

    /// Primer trozo corto: una cláusula o, como máximo, `first_chars`.
    fn emit_first_clause(&mut self) -> Option<String> {
        if !self.first || char_len(&self.buf) < 12 {
            return None;
        }
        let end = match first_clause(&self.buf) {
            Some(start) if start >= 8 => start + 1,
            _ if char_len(&self.buf) >= self.first_chars => {
                match rfind_space(&self.buf, self.first_chars) {
                    Some(cut) if cut > 0 => cut,
                    _ => return None,
                }
            }
            _ => return None,
        };
        let sentence = py_strip(head(&self.buf, end)).to_string();
        self.buf = py_lstrip(tail(&self.buf, end)).to_string();
        self.first = false;
        if sentence.is_empty() {
            None
        } else {
            Some(sentence)
        }
    }

    /// Añade un trozo y devuelve las frases completas que ya se pueden decir.
    pub fn feed(&mut self, delta: &str) -> Vec<String> {
        self.buf.push_str(delta);
        let mut out = Vec::new();
        if let Some(first) = self.emit_first_clause() {
            out.push(first);
        }
        while let Some(end) = sentence_end(&self.buf) {
            let sentence = py_strip(head(&self.buf, end)).to_string();
            self.buf = py_lstrip(tail(&self.buf, end)).to_string();
            if !sentence.is_empty() {
                out.push(sentence);
                self.first = false;
            }
        }
        if char_len(&self.buf) > self.max_chars {
            if let Some(cut) = rfind_space(&self.buf, self.max_chars) {
                if cut > 0 {
                    out.push(py_strip(head(&self.buf, cut)).to_string());
                    self.buf = py_lstrip(tail(&self.buf, cut)).to_string();
                    self.first = false;
                }
            }
        }
        out
    }

    /// Vacía el buffer; lo que queda, si algo queda, es una frase.
    pub fn flush(&mut self) -> Vec<String> {
        let text = py_strip(&self.buf).to_string();
        self.buf.clear();
        if text.is_empty() {
            Vec::new()
        } else {
            vec![text]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: &[&str], max_chars: usize, first_chars: usize) -> (Vec<String>, String) {
        let mut sentences = Sentences::new(max_chars, first_chars);
        let mut emitted = Vec::new();
        for chunk in chunks {
            emitted.extend(sentences.feed(chunk));
        }
        (emitted, sentences.buf.clone())
    }

    fn flushed(buffer: &str) -> Vec<String> {
        let mut sentences = Sentences {
            buf: buffer.to_string(),
            ..Sentences::default()
        };
        sentences.flush()
    }

    #[test]
    fn la_primera_frase_sale_en_la_coma() {
        assert_eq!(
            run(&["Perfecto, sigamos con el resto"], 160, 40).0,
            ["Perfecto,"]
        );
        assert_eq!(
            run(&["Escucha esto; es importante"], 160, 40).0,
            ["Escucha esto;"]
        );
        assert_eq!(
            run(&["  Primera frase con coma, y sigue el texto. "], 160, 40).0,
            ["Primera frase con coma,", "y sigue el texto."]
        );
    }

    #[test]
    fn una_coma_demasiado_pronto_no_corta() {
        assert!(run(&["Sí, claro que sí y más"], 160, 40).0.is_empty());
        assert_eq!(
            run(&["Escucha: esto es importante."], 160, 40).0,
            ["Escucha: esto es importante."]
        );
    }

    #[test]
    fn corta_por_espacio_antes_de_first_chars() {
        assert_eq!(
            run(
                &["Una frase bastante larga que", " sigue aquí. Otra."],
                160,
                40
            )
            .0,
            ["Una frase bastante larga que sigue", "aquí.", "Otra."]
        );
        assert_eq!(
            run(
                &["a b c d e f g h i j k l m n o p q r s t u v w x y z"],
                20,
                10
            )
            .0,
            ["a b c d e", "f g h i j k l m n o"]
        );
    }

    #[test]
    fn una_palabra_larga_sin_espacios_no_se_corta() {
        let long = "x".repeat(200);
        let (emitted, buffer) = run(&[&long], 40, 20);
        assert!(emitted.is_empty());
        assert_eq!(buffer, long);
    }

    #[test]
    fn cierra_frases_con_punto_interrogacion_o_exclamacion() {
        assert_eq!(run(&["Hola mundo. "], 160, 40).0, ["Hola mundo."]);
        assert_eq!(
            run(&["¿Qué hora es? No sé."], 160, 40).0,
            ["¿Qué hora es?", "No sé."]
        );
        assert_eq!(
            run(&["¡Claro que sí! Vamos"], 160, 40).0,
            ["¡Claro que sí!"]
        );
        assert_eq!(
            run(&["El Dr. Pérez llegó. Bien."], 160, 40).0,
            ["El Dr.", "Pérez llegó.", "Bien."]
        );
        assert_eq!(run(&["Uno... Dos"], 160, 40).0, ["Uno..."]);
    }

    #[test]
    fn los_decimales_y_las_urls_no_cortan_por_punto() {
        assert_eq!(
            run(&["El valor es 3.14159 y sube."], 160, 40).0,
            ["El valor es 3.14159 y sube."]
        );
        assert_eq!(
            run(&["El valor es 3. y sube"], 160, 40).0,
            ["El valor es 3."]
        );
        assert_eq!(
            run(&["Visita https://example.com ahora. Luego sigue."], 160, 40).0,
            ["Visita https://example.com ahora. Luego", "sigue."]
        );
    }

    #[test]
    fn el_trozo_largo_se_corta_por_el_ultimo_espacio() {
        let (emitted, buffer) = run(&["palabra ".repeat(20).as_str()], 40, 20);
        assert_eq!(
            emitted,
            ["palabra palabra", "palabra palabra palabra palabra palabra"]
        );
        assert_eq!(buffer.split_whitespace().count(), 13);
        assert_eq!(
            run(
                &["Texto ", "muy ", "largo ", "sin ", "punto ", "final"],
                20,
                40
            )
            .0,
            ["Texto muy largo sin"]
        );
    }

    #[test]
    fn los_chunks_se_acumulan_y_el_estado_queda_pendiente() {
        assert_eq!(
            run(&["Hola", " mundo", ". ¿Qué", " tal?"], 160, 40).0,
            ["Hola mundo.", "¿Qué tal?"]
        );
        let (emitted, buffer) = run(&["Uno\ndos\ntres"], 160, 40);
        assert!(emitted.is_empty());
        assert_eq!(buffer, "Uno\ndos\ntres");
    }

    #[test]
    fn los_limites_en_cero_se_respetan() {
        assert_eq!(run(&["Hola. Adiós."], 160, 0).0, ["Hola.", "Adiós."]);
        assert!(run(&["abc"], 0, 40).0.is_empty());
        let long = "🙂".repeat(10);
        assert!(run(&[&long], 5, 0).0.is_empty());
    }

    #[test]
    fn flush_devuelve_lo_pendiente_sin_espacios_sobrantes() {
        assert!(flushed("").is_empty());
        assert!(flushed("   ").is_empty());
        assert!(flushed("\n\n").is_empty());
        assert_eq!(flushed("  hola  "), ["hola"]);
        assert_eq!(flushed("hola\nmundo"), ["hola\nmundo"]);
        assert_eq!(flushed("Texto. "), ["Texto."]);
        assert_eq!(flushed("con\t\ttabulador"), ["con\t\ttabulador"]);
        assert_eq!(flushed("  ,  "), [","]);
        assert_eq!(flushed("emoji 🙂"), ["emoji 🙂"]);
    }

    #[test]
    fn flush_no_toca_la_bandera_de_primera_frase() {
        let mut sentences = Sentences {
            buf: "primera".to_string(),
            ..Sentences::default()
        };
        assert_eq!(sentences.flush(), ["primera"]);
        assert!(sentences.buf.is_empty());
        assert!(sentences.emit_first_clause().is_none());
    }

    #[test]
    fn los_espacios_unicode_de_python_separan() {
        assert_eq!(
            run(&["\u{1c}Hola.\u{1c}Adios."], 160, 40).0,
            ["Hola.", "Adios."]
        );
        assert_eq!(
            run(&["Hola \u{1c} mundo."], 160, 40).0,
            ["Hola \u{1c} mundo."]
        );
        assert_eq!(
            run(&["Primera.\nSegunda."], 160, 40).0,
            ["Primera.", "Segunda."]
        );
    }

    #[test]
    fn el_emoji_cuenta_como_un_punto_de_codigo() {
        let mut sentences = Sentences::new(10, 40);
        assert_eq!(
            sentences.feed("Mañana revisaré la ñ. ¡Hecho! 🙂"),
            ["Mañana revisaré la ñ.", "¡Hecho!"]
        );
        assert_eq!(sentences.flush(), ["🙂"]);
        let mut short = Sentences::new(2, 40);
        assert!(short.feed("🙂🙂🙂").is_empty());
        assert_eq!(short.buf, "🙂🙂🙂");
    }
}
