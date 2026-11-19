//! Lectura de las instrucciones que el árbitro entrega a un subagente (ECO-8).

use std::path::PathBuf;

/// Extrae la ruta del archivo de instrucciones de la primera línea del pez.
pub(crate) fn path_from_first_line(line: &str) -> Result<PathBuf, String> {
    let after = line
        .split_once("Lee ")
        .ok_or_else(|| format!("la línea no manda leer instrucciones: {line}"))?;
    let (path, _) = after
        .1
        .split_once(" y síguelo")
        .ok_or_else(|| format!("la línea no dice «y síguelo»: {line}"))?;
    let path = path.trim();
    if path.is_empty() {
        return Err("la ruta de instrucciones está vacía".to_string());
    }
    Ok(PathBuf::from(path))
}

/// Saca la ruta del informe del comando `ez report --file "..."` de las
/// instrucciones. Los valores entre comillas pueden contener espacios.
pub(crate) fn report_path(text: &str) -> Result<PathBuf, String> {
    let line = text
        .lines()
        .find(|line| line.contains("ez report"))
        .ok_or_else(|| "las instrucciones no traen el comando ez report".to_string())?;
    let words = tokenize(line);
    let index = words
        .iter()
        .position(|word| word == "--file")
        .ok_or_else(|| "el comando ez report no trae --file".to_string())?;
    let path = words
        .get(index + 1)
        .ok_or_else(|| "--file no tiene valor".to_string())?;
    if path.is_empty() {
        return Err("la ruta del informe está vacía".to_string());
    }
    Ok(PathBuf::from(path))
}

/// ¿Pide el papel las banderas de mutaciones del validador de QA?
pub(crate) fn asks_mutations(text: &str) -> bool {
    text.contains("--mutations-tried")
}

/// Id del pez, tomado de la cabecera «# Tu papel: ... (id, ronda n)».
pub(crate) fn fish_id(text: &str) -> Option<String> {
    let line = text.lines().find(|line| line.starts_with("# Tu papel:"))?;
    let open = line.find('(')? + 1;
    let end = line.get(open..)?.find(',')? + open;
    let id = line.get(open..end)?.trim();
    (!id.is_empty()).then(|| id.to_string())
}

fn tokenize(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in line.chars() {
        match quote {
            Some(mark) if ch == mark => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() && !current.is_empty() => {
                words.push(std::mem::take(&mut current));
            }
            None if ch.is_whitespace() => {}
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "Eres un subagente de EzAgent con el papel QA. Lee C:\\p\\qa-r1.instrucciones.md y síguelo al pie de la letra.";

    const INSTRUCTIONS: &str = "\
# Tu papel: QA (qa-r1, ronda 1)
Cuando termines ejecuta, en una sola línea:
`ez report --verdict <pass|fail> --file \"C:\\p\\ronda-1\\qa-r1.informe.md\" --blocking <n> --major <n> --minor <n>`
";

    #[test]
    fn reads_the_instructions_path_from_the_first_line() {
        assert_eq!(
            path_from_first_line(FIRST).expect("ruta"),
            PathBuf::from("C:\\p\\qa-r1.instrucciones.md")
        );
        assert!(path_from_first_line("hola").is_err());
        assert!(path_from_first_line("Lee  y síguelo").is_err());
    }

    #[test]
    fn extracts_the_report_path_even_with_quotes() {
        assert_eq!(
            report_path(INSTRUCTIONS).expect("informe"),
            PathBuf::from("C:\\p\\ronda-1\\qa-r1.informe.md")
        );
        assert!(report_path("sin comando").is_err());
        assert!(report_path("ez report --verdict pass").is_err());
    }

    #[test]
    fn detects_mutations_and_the_fish_id() {
        assert!(!asks_mutations(INSTRUCTIONS));
        assert!(asks_mutations(
            "ez report --mutations-tried 3 --mutations-caught 3"
        ));
        assert_eq!(fish_id(INSTRUCTIONS).as_deref(), Some("qa-r1"));
        assert_eq!(fish_id("sin cabecera"), None);
    }
}
