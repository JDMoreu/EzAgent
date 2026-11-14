//! `drop.py 250-362`: la cuadrícula del fluido, su render y la máscara exacta.

use std::cell::RefCell;
use std::collections::HashMap;

use ez_gfx::Mask;
use ndarray::{Array2, Array3};

use crate::model::drop::{DropModel, FlowState, CIRCLE_D, GRID_CELL, PILL_H, PILL_W};
use crate::paint::flow::FlowField;

/// `_GRIDS`: la cuadrícula gruesa de cada disposición se calcula una vez.
type Grid = (Array2<f32>, Array2<f32>);

thread_local! {
    static GRIDS: RefCell<HashMap<String, Grid>> = RefCell::new(HashMap::new());
    static MASKS: RefCell<HashMap<(String, i64), Mask>> = RefCell::new(HashMap::new());
}

/// `_flow_for`: el fluido de la forma, con la proporción real de la píldora.
pub(super) fn flow_field(state: &FlowState) -> FlowField {
    let mut field = FlowField::new(state.layout, Some(state.aspect), 7);
    field.t = state.t;
    field.pressure = state.pressure;
    field.wave = state.wave;
    field.lift = state.lift;
    field.angle = state.angle;
    field.spin_rate = state.spin_rate;
    field.mode.clone_from(&state.mode);
    field
}

/// `_form_dims`: el tamaño lógico de la forma de color.
pub(super) fn form_dims(layout: &str) -> (f64, f64) {
    if layout == "circle" {
        (CIRCLE_D, CIRCLE_D)
    } else {
        (PILL_W, PILL_H)
    }
}

/// `round(w / GRID_CELL)` con el redondeo a par de Python.
fn cells(size: f64) -> usize {
    (size / GRID_CELL).round_ties_even().max(2.0) as usize
}

fn axis(length: usize, size: f64, unit: f64) -> Vec<f32> {
    (0..length)
        .map(|index| (((index as f64 + 0.5) * (size / length as f64) - size / 2.0) / unit) as f32)
        .collect()
}

/// `_fluid_grid`: cuadrícula gruesa en unidades de media altura de la forma.
pub(super) fn fluid_grid(layout: &str) -> Grid {
    if let Some(grid) = GRIDS.with(|cache| cache.borrow().get(layout).cloned()) {
        return grid;
    }
    let (width, height) = form_dims(layout);
    let nx = cells(width);
    let ny = cells(height);
    let unit = height / 2.0;
    let xs = axis(nx, width, unit);
    let ys = axis(ny, height, unit);
    let gx = Array2::from_shape_fn((ny, nx), |(_, column)| xs[column]);
    let gy = Array2::from_shape_fn((ny, nx), |(row, _)| ys[row]);
    GRIDS.with(|cache| {
        cache
            .borrow_mut()
            .insert(layout.to_string(), (gx.clone(), gy.clone()));
    });
    (gx, gy)
}

/// Interior de la forma (el fluido): RGB uint8 en la cuadrícula gruesa.
///
/// `now` se conserva por paridad con el Python, que lo recibe y no lo usa.
// DECISION: la rejilla y la imagen se devuelven como tipos de `ndarray`
// (`Array2`/`Array3`), igual que el Python; la máscara es `ez_gfx::Mask` y no
// una `PIL.Image` de modo L.
pub fn render_fluid(model: &DropModel, _now: f64) -> Array3<u8> {
    let (gx, gy) = fluid_grid(model.layout);
    let image = flow_field(&model.field).render(&gx.view(), &gy.view(), model.theme, &model.cols);
    Array3::from_shape_fn(image.dim(), |(row, column, channel)| {
        (image[[row, column, channel]] * model.bright).clamp(0.0, 255.0) as u8
    })
}

/// Máscara exacta (modo L) de la forma a la resolución final.
pub fn form_mask(layout: &str, scale: f64) -> Mask {
    let key = (layout.to_string(), (scale * 100.0).round_ties_even() as i64);
    if let Some(mask) = MASKS.with(|cache| cache.borrow().get(&key).cloned()) {
        return mask;
    }
    let mask = build_mask(layout, scale);
    MASKS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 32 {
            // Crecimiento al acercar: usa varias escalas y la caché no crece sin fin.
            cache.clear();
        }
        cache.insert(key, mask.clone());
    });
    mask
}

fn build_mask(layout: &str, scale: f64) -> Mask {
    // NumPy mantiene float32 en todo el cálculo: los escalares de Python se
    // convierten a float32 (promoción débil de NumPy 2.x).
    let (width, height) = if layout == "circle" {
        let side = (CIRCLE_D * scale).round_ties_even().max(2.0) as usize;
        (side, side)
    } else {
        (
            (PILL_W * scale).round_ties_even().max(2.0) as usize,
            (PILL_H * scale).round_ties_even().max(2.0) as usize,
        )
    };
    let half_width = (width as f64 / 2.0) as f32;
    let half_height = (height as f64 / 2.0) as f32;
    let radius = height as f64 / 2.0;
    let inset = (width as f64 / 2.0 - radius) as f32;
    let mut data = Vec::with_capacity(width * height);
    for row in 0..height {
        let yy = (row as f32 + 0.5) - half_height;
        for column in 0..width {
            let xx = (column as f32 + 0.5) - half_width;
            let distance = if layout == "circle" {
                (xx * xx + yy * yy).sqrt() - half_width
            } else {
                let dx = (xx.abs() - inset).max(0.0);
                (dx * dx + yy * yy).sqrt() - radius as f32
            };
            let alpha = (0.5 - distance).clamp(0.0, 1.0);
            data.push((alpha * 255.0_f32) as u8);
        }
    }
    Mask::from_raw(width as u32, height as u32, data).expect("la máscara tiene su tamaño")
}
