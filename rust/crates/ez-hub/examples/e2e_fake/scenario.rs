//! Guion del agente principal falso: borradores y comandos `ez` en orden.

const ENCARGO: &str = "\
Demostrar el ciclo completo de EzAgent sin modelos de lenguaje. \
Un subagente ejecutor deja un archivo de prueba y los verificadores lo revisan.";

const CRITERIOS: &str = "\
[validacion]
mutaciones_minimas = 3
rondas_maximas = 5
horas_maximas = 2.0
profundidad_maxima = 3

[[criterio]]
id = \"C1\"
descripcion = \"Git está instalado en la máquina\"
comando = [\"git\", \"--version\"]
contiene = \"git version\"
";

const EQUIPO: &str = "\
[[papel]]
papel = \"planner\"
agente = \"fake_exec\"
modelo = \"exec-1\"

[[papel]]
papel = \"plan_reviewer\"
agente = \"fake_verify\"
modelo = \"verify-1\"
etapa = 5

[[papel]]
papel = \"developer\"
agente = \"fake_exec\"
modelo = \"exec-1\"

[[papel]]
papel = \"qa\"
agente = \"fake_verify\"
modelo = \"verify-1\"

[[papel]]
papel = \"qa_validator\"
agente = \"fake_verify\"
modelo = \"verify-1\"
";

fn bracket(args: &[&str]) -> String {
    let items: Vec<String> = args.iter().map(|arg| quoted(arg)).collect();
    format!("[{}]", items.join(", "))
}

/// Cadena basic de TOML, con las barras y comillas escapadas.
pub fn quoted(value: &str) -> String {
    let mut text = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => text.push_str("\\\\"),
            '"' => text.push_str("\\\""),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            _ => text.push(ch),
        }
    }
    text.push('"');
    text
}

fn draft(path: &str, content: &str) -> String {
    format!(
        "[[borrador]]\nruta = {}\ncontenido = {}\n",
        quoted(path),
        quoted(content)
    )
}

fn step_ez(args: &[&str]) -> String {
    format!("[[paso]]\nez = {}\n", bracket(args))
}

fn step_wait(kind: &str) -> String {
    format!("[[paso]]\nwait = {}\n", quoted(kind))
}

fn delegate(role: &str) -> String {
    let brief = format!(".ez/borrador/{role}.md");
    step_ez(&["delegate", role, "--brief", &brief])
}

/// Texto completo del guion: dos rondas, la segunda solo de verificación.
pub fn script() -> String {
    let mut text = String::new();
    text.push_str(&draft(".ez/borrador/encargo.md", ENCARGO));
    text.push_str(&draft(".ez/borrador/criterios.toml", CRITERIOS));
    text.push_str(&draft(".ez/borrador/equipo.toml", EQUIPO));
    for role in [
        "planner",
        "plan_reviewer",
        "developer",
        "qa",
        "qa_validator",
    ] {
        let brief = format!("Haz el papel {role} de esta tarea de prueba.");
        text.push_str(&draft(&format!(".ez/borrador/{role}.md"), &brief));
    }
    text.push_str(&step_ez(&[
        "task",
        "new",
        "--title",
        "Punta a punta con agentes falsos",
        "--brief",
        ".ez/borrador/encargo.md",
        "--criteria",
        ".ez/borrador/criterios.toml",
    ]));
    text.push_str(&step_wait("task_approved"));
    text.push_str(&step_ez(&[
        "team",
        "propose",
        "--file",
        ".ez/borrador/equipo.toml",
    ]));
    text.push_str(&step_wait("team_approved"));
    // DECISION: el revisor del plan va en la etapa 5 para que una ronda solo
    // de verificación no espere su informe (ver el informe ECO-8-r1).
    round(
        &mut text,
        &[
            "planner",
            "developer",
            "qa",
            "plan_reviewer",
            "qa_validator",
        ],
    );
    round(&mut text, &["qa", "qa_validator"]);
    text.push_str(&step_ez(&["close"]));
    text
}

fn round(text: &mut String, roles: &[&str]) {
    text.push_str(&step_ez(&["round", "start"]));
    for role in roles {
        text.push_str(&delegate(role));
        text.push_str(&step_wait("fish_reported"));
    }
    text.push_str(&step_ez(&["round", "close"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_has_two_rounds_and_a_close() {
        let text = script();
        assert_eq!(text.matches("ez = [\"round\", \"start\"]").count(), 2);
        assert_eq!(text.matches("ez = [\"round\", \"close\"]").count(), 2);
        assert!(text.contains("ez = [\"close\"]"));
        assert_eq!(text.matches("ez = [\"delegate\"").count(), 7);
        assert_eq!(text.matches("wait = \"fish_reported\"").count(), 7);
    }

    #[test]
    fn quoted_strings_are_valid_toml() {
        assert_eq!(quoted("C:\\p\\x.toml"), "\"C:\\\\p\\\\x.toml\"");
        assert_eq!(quoted("dice \"hola\""), "\"dice \\\"hola\\\"\"");
        assert!(text_has_all_drafts(&script()));
    }

    fn text_has_all_drafts(text: &str) -> bool {
        ["encargo.md", "criterios.toml", "equipo.toml"]
            .iter()
            .all(|name| text.contains(name))
    }
}
