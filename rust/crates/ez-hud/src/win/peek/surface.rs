//! Superficie de la ventana de la vista rápida (`P-FIX-LOCKAUDIT`).
//!
//! `Peek` publica estado bajo locks cortos y aplica los cambios de ventana
//! **sin** ninguno tomado: `ShowWindow`/`SetWindowPos` sobre una ventana de otro
//! hilo esperan a que ese hilo atienda el mensaje, y ese hilo puede estar en
//! `render` pidiendo los mismos locks. El trait existe para que la prueba de
//! regresión pueda poner una ventana que se comporte así (Win32) sin abrir una
//! ventana real; en producción solo lo implementa [`super::window::PeekWindow`].

/// Operaciones de la tarjeta; las llama el controlador desde cualquier hilo.
pub(super) trait PeekSurface: Send + Sync {
    /// Arranca la ventana y su hilo de pintura (bloquea hasta que nace).
    fn start(self: std::sync::Arc<Self>);
    /// Detiene el hilo de pintura y la ventana.
    fn stop(&self);
    fn show(&self);
    fn hide(&self);
    fn resize(&self, width: i32, height: i32);
    fn set_pos(&self, x: i32, y: i32);
    fn is_visible(&self) -> bool;
    fn size(&self) -> (i32, i32);
    fn position(&self) -> (i32, i32);
    /// Cadencia de pintura que fija el controlador.
    fn set_interval(&self, seconds: f64);
    /// Pinta cuanto antes.
    fn mark_dirty(&self);
}
