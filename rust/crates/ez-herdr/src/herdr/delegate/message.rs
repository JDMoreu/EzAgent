//! El primer mensaje del subagente (§7 del diseño).
//!
//! Lo escribe el código, nunca el modelo: el subagente recibe su papel, la
//! terminal de su padre y el encargo.

/// La plantilla del mensaje: `{rol}`, `{padre}` y `{encargo}`.
const TEMPLATE: &str = "Eres un subagente de EzAgent con el papel «{rol}», lanzado por la terminal {padre}.\n\
Trabaja solo en lo que dice tu encargo. No lances otros agentes salvo que tu encargo lo pida; si lo pide, usa `ez delegate`. Al terminar, escribe tu informe en un archivo y ejecuta: ez report --state terminado --file <informe> --summary \"<una frase>\". Si no puedes completarlo, ejecuta: ez report --state fallido --summary \"<motivo>\". Si necesitas preguntarle algo a tu agente, usa: hc send {padre} \"<pregunta>\".\n\
\n\
Encargo:\n\
{encargo}";

/// El mensaje con el papel, el padre y el encargo.
pub fn first_message(role: &str, parent: &str, brief: &str) -> String {
    TEMPLATE
        .replace("{rol}", role)
        .replace("{padre}", parent)
        .replace("{encargo}", brief)
}

#[cfg(test)]
mod tests {
    use super::first_message;

    #[test]
    fn el_mensaje_tiene_el_papel_el_padre_y_el_encargo() {
        let message = first_message("developer", "t6h", "arregla el parser");
        assert!(message.starts_with(
            "Eres un subagente de EzAgent con el papel «developer», lanzado por la terminal t6h."
        ));
        assert!(message.contains("usa `ez delegate`"));
        assert!(message.contains("ez report --state terminado --file <informe>"));
        assert!(message.contains("ez report --state fallido --summary \"<motivo>\""));
        assert!(message.contains("hc send t6h \"<pregunta>\""));
        assert!(message.ends_with("Encargo:\narregla el parser"));
    }

    #[test]
    fn el_mensaje_no_deja_plantillas_sin_rellenar() {
        let message = first_message("qa", "t9z", "encargo");
        assert!(!message.contains("{rol}"));
        assert!(!message.contains("{padre}"));
        assert!(!message.contains("{encargo}"));
    }
}
