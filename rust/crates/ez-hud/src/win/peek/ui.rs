//! Datos de la barra que consume la vista rápida (`DropUI`).

/// Datos de la barra que consume la vista rápida (`DropUI`).
/// La vista no conoce Tk ni la ventana de la barra: recibe la escala, el tema y
/// los avisos de actividad por este trait.
pub trait PeekUi: Send + Sync {
    /// `DropUI.bs`: escala de dibujo de la barra (`s * bar_scale`).
    fn bar_scale(&self) -> Option<f64>;
    /// `DropUI.s`: escala de pantalla.
    fn scale(&self) -> f64;
    /// `DropUI._theme_name()`: `"light"` o `"dark"`.
    fn theme_name(&self) -> String;
    /// `DropUI._reduced`: movimiento reducido.
    fn reduced(&self) -> bool {
        false
    }
    /// `DropUI.hub_extent()`: rectángulo del hub en pantalla (px físicos).
    fn hub_extent(&self) -> Option<(f64, f64, f64, f64)> {
        None
    }
    /// `DropUI.form_extent()`: centro de la forma y su medio alto.
    fn form_extent(&self) -> Option<(f64, f64, f64)> {
        None
    }
    /// Área de trabajo del escritorio (`layered.work_area()`).
    fn work_area(&self) -> (i32, i32, i32, i32) {
        crate::win::monitor::work_area()
    }
    /// `DropUI.on_activity_dismiss`: la app olvida el aviso en su hilo.
    fn on_activity_dismiss(&self, _key: &str) {}
    /// `DropUI.on_activity_open`: trae al frente la terminal del agente.
    fn on_activity_open(&self, _key: &str) {}
}
