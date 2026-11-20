//! El análisis de la línea de comandos de `ez` (argumentos, opciones y
//! banderas). Vive aparte para que `cli.rs` no crezca más allá del límite.

use serde_json::Value;

use crate::protocol::Request;

use super::{CliError, Command, FLAG_OPTIONS};

/// Los argumentos de la línea de comandos, ya analizados.
pub(crate) struct Parsed {
    pub(crate) words: Vec<String>,
    pub(crate) options: Vec<(String, String)>,
    pub(crate) flags: Vec<String>,
    pub(crate) json: bool,
}

impl Parsed {
    pub(crate) fn parse(args: &[String]) -> Result<Parsed, String> {
        let mut parsed = Parsed {
            words: Vec::new(),
            options: Vec::new(),
            flags: Vec::new(),
            json: false,
        };
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if arg == "--json" {
                parsed.json = true;
                index += 1;
                continue;
            }
            if let Some(name) = arg.strip_prefix("--") {
                // DECISION: `--no-notify` y compañía no llevan valor; sin esto
                // el análisis genérico se comería la opción siguiente.
                if FLAG_OPTIONS.contains(&name) {
                    parsed.flags.push(name.to_string());
                    index += 1;
                    continue;
                }
                let value = args.get(index + 1).filter(|value| !value.starts_with("--"));
                let Some(value) = value else {
                    return Err(format!("Falta el valor de --{name}."));
                };
                parsed.options.push((name.to_string(), value.clone()));
                index += 2;
                continue;
            }
            parsed.words.push(arg.clone());
            index += 1;
        }
        Ok(parsed)
    }

    pub(crate) fn word(&mut self, missing: &str) -> Result<String, CliError> {
        if self.words.is_empty() {
            return Err(CliError::Usage(missing.to_string()));
        }
        Ok(self.words.remove(0))
    }

    pub(crate) fn take(&mut self, name: &str) -> Option<String> {
        let index = self.options.iter().position(|(key, _)| key == name)?;
        Some(self.options.remove(index).1)
    }

    /// ¿Aparece la opción? (sin consumirla).
    pub(crate) fn has(&self, name: &str) -> bool {
        self.options.iter().any(|(key, _)| key == name)
    }

    /// ¿Aparece la opción sin valor? (y la consume)
    pub(crate) fn flag(&mut self, name: &str) -> bool {
        let Some(index) = self.flags.iter().position(|flag| flag == name) else {
            return false;
        };
        self.flags.remove(index);
        true
    }

    pub(crate) fn required(&mut self, name: &str) -> Result<String, CliError> {
        self.take(name)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CliError::Usage(format!("Falta --{name}.")))
    }

    pub(crate) fn number(&mut self, name: &str) -> Result<Option<u64>, CliError> {
        let Some(value) = self.take(name) else {
            return Ok(None);
        };
        let number = value
            .trim()
            .parse::<u64>()
            .map_err(|_| CliError::Usage(format!("--{name} debe ser un número entero.")))?;
        Ok(Some(number))
    }

    pub(crate) fn required_number(&mut self, name: &str) -> Result<u64, CliError> {
        self.number(name)?
            .ok_or_else(|| CliError::Usage(format!("Falta --{name}.")))
    }

    /// ¿Sobró algo?
    pub(crate) fn check_empty(&mut self) -> Result<(), CliError> {
        if let Some(word) = self.words.first() {
            return Err(CliError::Usage(format!("Argumento de más: {word}.")));
        }
        if let Some((name, _)) = self.options.first() {
            return Err(CliError::Usage(format!("Opción desconocida: --{name}.")));
        }
        if let Some(name) = self.flags.first() {
            return Err(CliError::Usage(format!("Opción desconocida: --{name}.")));
        }
        Ok(())
    }

    /// La orden para el árbitro.
    pub(crate) fn finish_request(
        &mut self,
        command: &str,
        args: Value,
    ) -> Result<Request, CliError> {
        self.check_empty()?;
        Ok(Request {
            command: command.to_string(),
            args,
        })
    }

    pub(crate) fn finish(&mut self, command: &str, args: Value) -> Result<Command, CliError> {
        let request = self.finish_request(command, args)?;
        Ok(Command::Arbiter(request))
    }
}
