//! Pasos del enjambre de F7-01c (escenarios 18-20).

use ez_core::pyvalue::python_str;
use serde_json::{json, Map, Value};

use super::super::driver::{ready, Driver, Pending, Step};
use super::super::swarm_c;
use super::{describe, integer, object};

/// `swarm_start: {goal, members}`.
pub struct SwarmStart;

impl Step for SwarmStart {
    fn immediate(&self) -> bool {
        false
    }

    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("swarm_start").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "swarm_start",
            step,
            step.get("swarm_start").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let block = object(step, "swarm_start");
        let stack = swarm_c::stack()?;
        let specs: Vec<_> = block
            .get("members")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(swarm_c::member_spec)
            .collect();
        stack
            .server
            .start()
            .map_err(|error| format!("no pude arrancar el enjambre: {error}"))?;
        let goal = block.get("goal").map(python_str).unwrap_or_default();
        let (swarm, _) = stack
            .service
            .start(&goal, &specs, "", None)
            .map_err(|error| error.to_string())?;
        swarm_c::set_swarm_id(swarm.clone());
        driver.trace.set_scrub(&swarm);
        driver.trace.record(
            "swarm.start",
            json!({
                "goal": goal,
                "members": specs.iter().map(|spec| spec.name.clone()).collect::<Vec<_>>(),
            }),
        );
        Ok(ready())
    }
}

/// `swarm_tick: {}` o `{times: 3}`.
pub struct SwarmTick;

impl Step for SwarmTick {
    fn immediate(&self) -> bool {
        false
    }

    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("swarm_tick")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "swarm_tick",
            step,
            step.get("swarm_tick").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let stack = swarm_c::stack()?;
        let times = step
            .get("swarm_tick")
            .and_then(Value::as_object)
            .and_then(|spec| spec.get("times"))
            .map_or(1, |value| integer(Some(value)).max(1));
        let swarm = swarm_c::swarm_id();
        let before: Vec<(String, String)> = stack
            .store()
            .members(&swarm)
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|member| !member.address.is_empty())
            .map(|member| (member.name, member.address))
            .collect();
        let mut before = before;
        for _ in 0..times.max(1) {
            stack.supervisor.tick().map_err(|error| error.to_string())?;
            swarm_c::settle_launches(&stack, 30.0)?;
            let after_members = stack
                .store()
                .members(&swarm)
                .map_err(|error| error.to_string())?;
            let mut after = Vec::new();
            let mut fresh = Vec::new();
            for member in after_members {
                if member.address.is_empty() {
                    continue;
                }
                let item = (member.name.clone(), member.address.clone());
                if !before.contains(&item) {
                    fresh.push(member.name);
                }
                after.push(item);
            }
            if !fresh.is_empty() {
                driver
                    .trace
                    .record("swarm.launched", json!({ "members": fresh }));
                swarm_c::push_launch(fresh);
            }
            before = after;
        }
        Ok(ready())
    }
}

/// `swarm_agent: {member, argv}`.
pub struct SwarmAgent;

impl Step for SwarmAgent {
    fn immediate(&self) -> bool {
        false
    }

    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("swarm_agent").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "swarm_agent",
            step,
            step.get("swarm_agent").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let spec = object(step, "swarm_agent");
        let member = spec.get("member").map(python_str).unwrap_or_default();
        let argv: Vec<String> = spec
            .get("argv")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(python_str)
            .collect();
        let stack = swarm_c::stack()?;
        let (ok, output) = swarm_c::run_ezs(&stack, &member, &argv)?;
        let joined = argv.join(" ");
        driver.trace.record(
            "swarm.ezs",
            json!({ "member": member, "argv": joined, "ok": ok, "text": output }),
        );
        if !ok {
            return Err(format!("ezs {member} {joined}: {output}"));
        }
        if argv
            .first()
            .is_some_and(|word| ["done", "fix", "fail"].contains(&word.to_lowercase().as_str()))
        {
            swarm_c::update_terminal(&member, &[("state", Value::String("idle".to_string()))])?;
        }
        Ok(ready())
    }
}

/// `swarm_screen: {member, state, screen, agent}`.
pub struct SwarmScreen;

impl Step for SwarmScreen {
    fn immediate(&self) -> bool {
        false
    }

    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("swarm_screen").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "swarm_screen",
            step,
            step.get("swarm_screen").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let spec = object(step, "swarm_screen");
        let member = spec.get("member").map(python_str).unwrap_or_default();
        let fields: Vec<(&str, Value)> = ["state", "screen", "agent"]
            .iter()
            .filter_map(|key| spec.get(*key).map(|value| (*key, value.clone())))
            .collect();
        swarm_c::update_terminal(&member, &fields)?;
        let state = spec.get("state").map(python_str).unwrap_or_default();
        driver
            .trace
            .record("swarm.screen", json!({ "member": member, "state": state }));
        Ok(ready())
    }
}

/// `swarm_lost: {member, mode}`.
pub struct SwarmLost;

impl Step for SwarmLost {
    fn immediate(&self) -> bool {
        false
    }

    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("swarm_lost").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "swarm_lost",
            step,
            step.get("swarm_lost").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let spec = object(step, "swarm_lost");
        let member = spec.get("member").map(python_str).unwrap_or_default();
        let mode = spec
            .get("mode")
            .map(python_str)
            .filter(|mode| !mode.is_empty())
            .unwrap_or_else(|| "remove".to_string());
        match mode.as_str() {
            "remove" => {
                let stack = swarm_c::stack()?;
                let address = swarm_c::address(&stack, &member)?;
                let mut data = swarm_c::hc_state()?;
                if let Some(terminals) = data.get_mut("terminals").and_then(Value::as_array_mut) {
                    terminals.retain(|item| {
                        python_str(item.get("address").unwrap_or(&Value::Null)) != address
                    });
                }
                swarm_c::hc_save(&data)?;
            }
            "shell" => {
                swarm_c::update_terminal(
                    &member,
                    &[
                        ("agent", Value::String("powershell.exe".to_string())),
                        ("state", Value::String("unknown".to_string())),
                        ("screen", Value::String("PS C:\\> ".to_string())),
                    ],
                )?;
            }
            _ => {
                let screen = spec
                    .get("screen")
                    .map(python_str)
                    .filter(|screen| !screen.is_empty())
                    .unwrap_or_else(|| "error".to_string());
                swarm_c::update_terminal(
                    &member,
                    &[
                        ("state", Value::String("error".to_string())),
                        ("screen", Value::String(screen)),
                    ],
                )?;
            }
        }
        driver
            .trace
            .record("swarm.lost", json!({ "member": member, "mode": mode }));
        Ok(ready())
    }
}

/// `swarm_resolve: {incident?, choice}`.
pub struct SwarmResolve;

impl Step for SwarmResolve {
    fn immediate(&self) -> bool {
        false
    }

    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("swarm_resolve").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "swarm_resolve",
            step,
            step.get("swarm_resolve").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let spec = object(step, "swarm_resolve");
        let stack = swarm_c::stack()?;
        let mut incident = integer(spec.get("incident"));
        if incident == 0 {
            let open = stack
                .store()
                .incidents(&swarm_c::swarm_id(), true)
                .map_err(|error| error.to_string())?;
            let first = open
                .first()
                .ok_or_else(|| "no hay incidente abierto".to_string())?;
            incident = first.id;
        }
        let choice = spec.get("choice").map(python_str).unwrap_or_default();
        let text = stack
            .recovery()
            .resolve(incident, &choice, "", "", "")
            .map_err(|error| error.to_string())?;
        driver.trace.record(
            "swarm.resolve",
            json!({ "incident": incident, "choice": choice, "text": text }),
        );
        Ok(ready())
    }
}

/// `swarm_clock: {advance}`.
pub struct SwarmClock;

impl Step for SwarmClock {
    fn immediate(&self) -> bool {
        false
    }

    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("swarm_clock").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe(
            "swarm_clock",
            step,
            step.get("swarm_clock").unwrap_or(&Value::Null),
        )
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let advance = step
            .get("swarm_clock")
            .and_then(Value::as_object)
            .and_then(|spec| spec.get("advance"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        swarm_c::advance_clock(advance);
        driver
            .trace
            .record("swarm.clock", json!({ "advanced": advance }));
        Ok(ready())
    }
}
