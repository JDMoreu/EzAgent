//! Codificación de lo que la persona escribe en la barra de entrada.
//!
//! Una línea se envía tal cual y un salto de línea; varias líneas van como pegado
//! entre corchetes para que el agente no las ejecute tecla a tecla (§10).

/// Marca de inicio de pegado entre corchetes (bracketed paste).
pub const PASTE_START: &str = "\u{1b}[200~";
/// Marca de fin de pegado entre corchetes.
pub const PASTE_END: &str = "\u{1b}[201~";

/// Convierte el texto de la barra en los bytes que recibe la terminal.
///
/// Si el texto tiene saltos de línea se envuelve en `ESC[200~ … ESC[201~` y luego
/// se manda `\r`; si no, se envía el texto y `\r`.
pub fn encode_input(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 20);
    if text.contains('\n') {
        out.extend_from_slice(PASTE_START.as_bytes());
        // `\r\n` dentro del pegado insertaría un salto doble en la mayoría de TUI.
        out.extend_from_slice(text.replace("\r\n", "\n").as_bytes());
        out.extend_from_slice(PASTE_END.as_bytes());
    } else {
        out.extend_from_slice(text.trim_end_matches('\r').as_bytes());
    }
    out.push(b'\r');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_ends_with_carriage_return() {
        assert_eq!(encode_input("hola"), b"hola\r");
    }

    #[test]
    fn multiline_is_bracketed_then_submitted() {
        assert_eq!(
            encode_input("uno\ndos"),
            b"\x1b[200~uno\ndos\x1b[201~\r"
        );
    }

    #[test]
    fn windows_newlines_are_normalized_inside_the_paste() {
        assert_eq!(
            encode_input("uno\r\ndos"),
            b"\x1b[200~uno\ndos\x1b[201~\r"
        );
    }

    #[test]
    fn empty_text_is_just_the_enter_key() {
        assert_eq!(encode_input(""), b"\r");
    }
}
