//! Layout y geometría derivada del menú (F5-07).
//!
//! Misma tabla que `menu_layout`, pero filtrando los grupos visibles según los
//! ajustes; `layout::layout_menu` exige rebanadas `'static`.

use std::collections::BTreeMap;

use crate::layout;
use crate::tokens::{H_ISLAND, T_SLOW};

use super::{
    Menu, MenuGroup, MenuGroups, BLOCK_OFFSET, INDICATOR_SHOW_MS, SCROLL_INDICATOR_FADE_MS,
    SCROLL_INDICATOR_HOLD_MS,
};

/// Separación entre la cabecera y el contenido (`layout.GAP_HEADER`).
pub(crate) const GAP_HEADER: f64 = layout::GAP_HEADER as f64;
/// `T_SLOW` como `f64` para el tween de apertura.
pub(crate) const T_SLOW_MS: f64 = T_SLOW as f64;

pub(crate) fn default_groups() -> MenuGroups {
    layout::GROUPS
        .iter()
        .map(|&(name, rows)| {
            let rows = rows
                .iter()
                .map(|&(key, kind)| (key.to_string(), kind.to_string()))
                .collect();
            (name.to_string(), rows)
        })
        .collect()
}

pub(crate) fn block_delays() -> BTreeMap<String, f64> {
    let mut out = BTreeMap::from([("header".to_string(), 60.0)]);
    for (index, &(name, _rows)) in layout::GROUPS.iter().enumerate() {
        out.insert(name.to_string(), 100.0 + 40.0 * index as f64);
    }
    out.insert(
        "footer".to_string(),
        100.0 + 40.0 * layout::GROUPS.len() as f64,
    );
    out
}

impl Menu {
    /// Altura extra por selector abierto.
    pub fn extra(&self) -> BTreeMap<String, f64> {
        match &self.selector_key {
            None => BTreeMap::new(),
            Some(key) => BTreeMap::from([(key.clone(), self.selector_extra.value.max(0.0))]),
        }
    }

    /// Layout con el viewport objetivo; el pie queda fijo y el resto scrollea.
    pub fn layout(&self) -> BTreeMap<String, layout::Rect> {
        let extra = self.extra();
        self.layout_with(Some(&extra))
    }

    /// Igual que [`Menu::layout`] con un mapa `extra` explícito.
    pub fn layout_with(
        &self,
        extra: Option<&BTreeMap<String, f64>>,
    ) -> BTreeMap<String, layout::Rect> {
        let owned = extra.is_none().then(|| self.extra());
        let extra = extra.or(owned.as_ref());
        menu_layout(
            extra,
            Some(H_ISLAND as f64 + self.viewport_height()),
            &self.shown_groups(),
        )
    }

    /// Layout completo sin recortar; sirve para calcular el scroll máximo.
    pub fn nominal_layout(&self) -> BTreeMap<String, layout::Rect> {
        let extra = self.extra();
        self.nominal_layout_with(Some(&extra))
    }

    /// Igual que [`Menu::nominal_layout`] con un mapa `extra` explícito.
    pub fn nominal_layout_with(
        &self,
        extra: Option<&BTreeMap<String, f64>>,
    ) -> BTreeMap<String, layout::Rect> {
        let owned = extra.is_none().then(|| self.extra());
        let extra = extra.or(owned.as_ref());
        menu_layout(extra, None, &self.shown_groups())
    }

    /// Píxeles de desplazamiento disponibles con el contenido actual.
    pub fn scroll_max(&self) -> f64 {
        let extra = self.extra();
        let actual = self.layout_with(Some(&extra));
        let nominal = self.nominal_layout_with(Some(&extra));
        (footer(&nominal).y0 - footer(&actual).y0).max(0.0)
    }

    /// Desplazamiento actual, recortado al máximo.
    pub fn scroll_offset(&self) -> f64 {
        self.scroll_tween.value.max(0.0).min(self.scroll_max())
    }

    /// Borde superior del contenido desplazable.
    pub fn content_top(&self, layout: &BTreeMap<String, layout::Rect>) -> f64 {
        header(layout).y1 + GAP_HEADER
    }

    /// Borde superior del pie.
    pub fn footer_top(&self, layout: &BTreeMap<String, layout::Rect>) -> f64 {
        footer(layout).y0
    }

    /// `(alfa, desplazamiento_y)` de un bloque durante A-04.
    pub fn block_values(&self, name: &str, _now: f64) -> (f64, f64) {
        let raw = self
            .blocks
            .get(name)
            .map_or(0.0, |tween| tween.value.clamp(0.0, 1.0));
        (raw * self.content_alpha.value, (1.0 - raw) * BLOCK_OFFSET)
    }

    /// Registra actividad de scroll: el indicador se enciende y se apaga solo.
    pub fn indicator_activity(&mut self, now: f64) {
        self.indicator_last = now;
        self.indicator_until = now + SCROLL_INDICATOR_HOLD_MS / 1000.0;
    }

    /// Alfa del indicador de scroll en `now`.
    pub fn indicator_alpha(&self, now: f64) -> f64 {
        if self.scroll_max() <= 0.5 {
            return 0.0;
        }
        if now < self.indicator_last {
            return 0.0;
        }
        if now <= self.indicator_until {
            return ((now - self.indicator_last) / (INDICATOR_SHOW_MS / 1000.0)).min(1.0);
        }
        (1.0 - (now - self.indicator_until) / (SCROLL_INDICATOR_FADE_MS / 1000.0)).max(0.0)
    }
}

pub(crate) fn header(layout: &BTreeMap<String, layout::Rect>) -> layout::Rect {
    *layout
        .get("header")
        .expect("el layout siempre tiene cabecera")
}

pub(crate) fn footer(layout: &BTreeMap<String, layout::Rect>) -> layout::Rect {
    *layout.get("footer").expect("el layout siempre tiene pie")
}

/// Misma tabla que `layout_menu` con grupos ya filtrados.
///
/// // DECISION: `layout::layout_menu` exige rebanadas `'static`; los grupos
/// // visibles se filtran por ajustes, así que se recorre la tabla aquí
/// // reutilizando sus constantes y `row_height`.
fn menu_layout(
    extra: Option<&BTreeMap<String, f64>>,
    viewport_h: Option<f64>,
    groups: &[MenuGroup],
) -> BTreeMap<String, layout::Rect> {
    let mut out = BTreeMap::new();
    let mut y = layout::ISLAND_H as f64 + layout::PAD_TOP as f64;
    out.insert(
        "header".to_string(),
        layout::Rect::new(0.0, y, 0.0, y + layout::HEADER as f64),
    );
    y += layout::HEADER as f64 + GAP_HEADER;
    for (name, rows) in groups {
        out.insert(
            format!("label_{name}"),
            layout::Rect::new(0.0, y, 0.0, y + layout::LABEL as f64),
        );
        y += layout::LABEL as f64;
        let group_top = y;
        for (key, kind) in rows {
            let base = layout::row_height(kind).expect("tipo de fila conocido") as f64;
            let extra_height = extra
                .and_then(|values| values.get(key))
                .copied()
                .unwrap_or(0.0);
            let height = base + extra_height;
            out.insert(key.clone(), layout::Rect::new(0.0, y, 0.0, y + height));
            y += height;
        }
        out.insert(
            format!("group_{name}"),
            layout::Rect::new(0.0, group_top, 0.0, y),
        );
        y += layout::GROUP_GAP as f64;
    }
    let mut footer_top = y;
    let mut footer_bottom = y + layout::FOOTER as f64;
    if let Some(viewport) = viewport_h {
        footer_bottom = viewport - layout::PAD_BOTTOM as f64;
        footer_top = footer_bottom - layout::FOOTER as f64;
    }
    out.insert(
        "footer".to_string(),
        layout::Rect::new(0.0, footer_top, 0.0, footer_bottom),
    );
    y = footer_bottom + layout::PAD_BOTTOM as f64;
    out.insert("total".to_string(), layout::Rect::new(0.0, 0.0, 0.0, y));
    out
}
