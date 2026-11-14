//! Decisiones del usuario sobre un incidente: esperar, reemplazar, reabrir,
//! liberar o pausar.

use ez_core::pyvalue::{py_strip, repr_str};

use super::{Recovery, CHOICES};
use crate::db::DbResult;
use crate::incidents as inc;
use crate::models::{Incident, Member, SwarmError, SYSTEM};

impl Recovery {
    /// Aplica la decisión del usuario y devuelve qué se hizo, en una frase.
    pub fn resolve(
        &self,
        incident_id: i64,
        choice: &str,
        agent: &str,
        model: &str,
        effort: &str,
    ) -> DbResult<String> {
        let incident = self.store().incident(incident_id)?;
        let Some(incident) = incident.filter(|incident| incident.state != "resolved") else {
            return Err(
                SwarmError::new(format!("No hay un incidente abierto I{incident_id}.")).into(),
            );
        };
        let choice = py_strip(choice).to_lowercase();
        if !CHOICES.contains(&choice.as_str()) {
            return Err(SwarmError::new(format!(
                "Opción desconocida: {}. Usa: {}.",
                repr_str(&choice),
                CHOICES.join(", ")
            ))
            .into());
        }
        let member = self.member(&incident.swarm, &incident.member)?;
        match choice.as_str() {
            "wait" => self.wait(&incident, &member),
            "pause" => self.pause(&incident),
            "release" => self.release(&incident, &member),
            "reopen" => self.reopen(&incident, &member),
            _ => self.replace(&incident, &member, agent, model, effort),
        }
    }

    fn wait(&self, incident: &Incident, member: &Member) -> DbResult<String> {
        self.store().set_incident(incident.id, "waiting", "wait")?;
        let until = inc::when(incident.resets, incident.created);
        let suffix = if until.is_empty() {
            String::new()
        } else {
            format!(" hasta {until}")
        };
        Ok(format!("Espero a {}{suffix}; el resto sigue.", member.name))
    }

    fn pause(&self, incident: &Incident) -> DbResult<String> {
        self.store().set_incident(incident.id, "waiting", "pause")?;
        self.service.pause(&incident.swarm)?;
        Ok("Enjambre en pausa. Dime «retoma el enjambre» para seguir.".to_string())
    }

    fn reopen(&self, incident: &Incident, member: &Member) -> DbResult<String> {
        self.handoff(
            incident,
            member,
            &member.agent,
            "Se reabrió tu sesión",
            None,
            None,
        )?;
        self.store()
            .set_incident(incident.id, "resolved", "reopen")?;
        Ok(format!(
            "Reabro a {} con {}; retoma con su avance guardado.",
            member.name, member.agent
        ))
    }

    /// Devuelve sus tareas al rol para que las tome otro miembro.
    fn release(&self, incident: &Incident, member: &Member) -> DbResult<String> {
        let released = self.store().release_tasks(&incident.swarm, &member.name)?;
        self.store()
            .set_member_state(&incident.swarm, &member.name, "gone")?;
        self.store()
            .set_incident(incident.id, "resolved", "release")?;
        let names = if released.is_empty() {
            "nada pendiente".to_string()
        } else {
            released
                .iter()
                .map(|task| format!("T{task}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        Ok(format!(
            "{} sale del enjambre; su trabajo ({names}) queda para su rol.",
            member.name
        ))
    }

    fn replace(
        &self,
        incident: &Incident,
        member: &Member,
        agent: &str,
        model: &str,
        effort: &str,
    ) -> DbResult<String> {
        let alt = self.suggestion(&incident.swarm, member)?;
        let wanted = if agent.is_empty() {
            alt.as_ref().map_or(String::new(), |alt| alt.agent.clone())
        } else {
            agent.to_string()
        };
        let key = if wanted.is_empty() {
            None
        } else {
            self.host.resolve_agent(&wanted)
        };
        let Some(key) = key else {
            return Err(SwarmError::new(format!(
                "Indica con qué agente lo reemplazo (no conozco {}).",
                repr_str(&wanted)
            ))
            .into());
        };
        let (model, effort) = Self::replacement_values(agent, model, effort, alt.as_ref());
        let old = member.agent.clone();
        self.handoff(
            incident,
            member,
            &key,
            &format!("Reemplazas a {old}"),
            Some(&model),
            Some(&effort),
        )?;
        self.store()
            .set_incident(incident.id, "resolved", "replace")?;
        let shown = if model.is_empty() {
            key.clone()
        } else {
            format!("{key} {model}")
        };
        Ok(format!(
            "Reemplazo a {}: {} sale y entra {shown}, con el avance guardado.",
            member.name, old
        ))
    }

    /// Modelo y esfuerzo: los pedidos o, si no hay agente explícito, los del sugerido.
    fn replacement_values(
        agent: &str,
        model: &str,
        effort: &str,
        alt: Option<&Member>,
    ) -> (String, String) {
        let Some(alt) = alt.filter(|_| agent.is_empty()) else {
            return (model.to_string(), effort.to_string());
        };
        let model = if model.is_empty() {
            alt.model.clone()
        } else {
            model.to_string()
        };
        let effort = if effort.is_empty() {
            alt.effort.clone()
        } else {
            effort.to_string()
        };
        (model, effort)
    }

    /// Guarda el avance, cambia (o reabre) el agente y le deja la nota de traspaso.
    pub(super) fn handoff(
        &self,
        incident: &Incident,
        member: &Member,
        agent: &str,
        note: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> DbResult<()> {
        let path = if member.address.is_empty() {
            String::new()
        } else {
            self.snapshot(&incident.swarm, member)?
        };
        self.store().update_member(
            &incident.swarm,
            &member.name,
            agent,
            model.unwrap_or(&member.model),
            effort.unwrap_or(&member.effort),
        )?;
        let tasks = self.tasks(&incident.swarm, &member.name)?;
        let pending = if tasks.is_empty() {
            "tu trabajo".to_string()
        } else {
            tasks.join(", ")
        };
        let mut text = format!("{note}: retoma {pending}.");
        if !path.is_empty() {
            text.push_str(
                " El avance de la sesión anterior está en el archivo \
                 (léelo solo si lo necesitas).",
            );
        }
        self.store()
            .post(&incident.swarm, SYSTEM, &text, &member.name, "info", &path)?;
        (self.reset_launches)(&incident.swarm, &member.name);
        Ok(())
    }
}
