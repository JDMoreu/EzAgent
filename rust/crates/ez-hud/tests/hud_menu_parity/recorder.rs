//! Callbacks del menú que apuntan lo que ocurre en cada paso.

use std::collections::BTreeSet;
use std::sync::Arc;

use ez_hud::model::menu::geometry::MenuCallbacks;
use ez_hud::model::settings::SettingsValues;
use ez_parity::fixtures::FixtureCase;
use parking_lot::Mutex;

use super::{a, b, o, s, val};

#[derive(Clone)]
pub(crate) struct Recorder {
    calls: Arc<Mutex<Vec<String>>>,
    saved: Arc<Mutex<SettingsValues>>,
    mods: Arc<Mutex<BTreeSet<String>>>,
    save: Option<bool>,
    copy: bool,
}

impl Recorder {
    pub(crate) fn new(case: &FixtureCase, saved: SettingsValues) -> Self {
        let save = if case.case.contains("save-pending") {
            None
        } else if case.case.ends_with("save-fail")
            || case.case.contains("save-error")
            || case.case.contains("footer-error")
        {
            Some(false)
        } else {
            Some(true)
        };
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            saved: Arc::new(Mutex::new(saved)),
            mods: Arc::new(Mutex::new(BTreeSet::new())),
            save,
            copy: true,
        }
    }

    pub(crate) fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.calls.lock())
    }

    pub(crate) fn set_mods(&self, names: &[&str]) {
        let mut mods = self.mods.lock();
        mods.clear();
        mods.extend(names.iter().map(|name| (*name).to_string()));
    }

    fn push(&self, call: String) {
        self.calls.lock().push(call);
    }

    fn diff(&self, values: &SettingsValues) -> String {
        let saved = self.saved.lock();
        let changed: Vec<(&str, String)> = values
            .iter()
            .filter(|(key, value)| saved.get(key) != Some(*value))
            .map(|(key, value)| (key, val(value)))
            .collect();
        o(&changed)
    }

    pub(crate) fn callbacks(&self) -> MenuCallbacks {
        let live = self.clone();
        let save = self.clone();
        let test = self.clone();
        let hide = self.clone();
        let close = self.clone();
        let text_edit = self.clone();
        let action = self.clone();
        let copy = self.clone();
        let capture = self.clone();
        let mods = self.clone();
        MenuCallbacks {
            live_change: Some(Box::new(move |key, value| {
                live.push(a(&[s("live"), s(key), val(value)]));
            })),
            save: Some(Box::new(move |values| {
                save.push(a(&[
                    s("save"),
                    save.save.map_or_else(|| "null".into(), b),
                    save.diff(values),
                ]));
                save.save
            })),
            test: Some(Box::new(move |values| {
                test.push(a(&[s("test"), test.diff(values)]));
            })),
            hide: Some(Box::new(move || hide.push(a(&[s("hide")])))),
            close: Some(Box::new(move || close.push(a(&[s("close")])))),
            text_edit: Some(Box::new(move |active| {
                text_edit.push(a(&[s("text_edit"), b(active)]));
            })),
            action: Some(Box::new(move |key| {
                action.push(a(&[s("action"), s(key)]));
            })),
            copy: Some(Box::new(move |text| {
                copy.push(a(&[s("copy"), s(text)]));
                copy.copy
            })),
            hotkey_capture: Some(Box::new(move |active| {
                capture.push(a(&[s("hotkey_capture"), b(active)]));
            })),
            modifier_source: Some(Box::new(move || mods.mods.lock().clone())),
        }
    }
}
