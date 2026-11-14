//! Port of `src/ezagent/voice/hud/layered.py` `frame_due`..`paint_mark` (F2-12).
//!
//! Ritmo del bucle de dibujo de una ventana layered: cuándo toca pintar,
//! cuánto esperar mensajes y qué hora tomar como referencia del fotograma.

/// Margen del reloj: Windows despierta en milisegundos enteros. Sin margen, un
/// despertar a los 16,4 ms con un intervalo de 16,7 ms (60 fps) saltaba el
/// fotograma y lo pintaba a los 33 ms: un tirón cada pocos cuadros.
pub const FRAME_SLACK: f64 = 0.002;

/// ¿Toca pintar? Admite despertar hasta `FRAME_SLACK` antes de la hora.
pub fn frame_due(now: f64, last_paint: f64, interval: f64) -> bool {
    now - last_paint >= interval - FRAME_SLACK
}

/// ¿Hay que pintar ahora? Nunca mientras se arrastra una ventana `lazy_drag`.
pub fn paint_due(
    visible: bool,
    dragging: bool,
    dirty: bool,
    now: f64,
    last_paint: f64,
    interval: f64,
) -> bool {
    if !visible || dragging {
        return false;
    }
    dirty || frame_due(now, last_paint, interval)
}

/// Cuánto esperar mensajes antes de la siguiente vuelta del bucle.
///
/// Oculta no pinta y `last_paint` no avanza: sin este caso el retardo quedaba
/// negativo y el bucle giraba sin dormir, acaparando el GIL (medido: cuadros de
/// la barra de 2 a 50-90 ms). Mostrarla manda mensajes al hilo, que despierta.
pub fn wait_delay(visible: bool, last_paint: f64, interval: f64, now: f64) -> f64 {
    if !visible {
        return 1.0 / 60.0;
    }
    (1.0_f64 / 60.0).min(last_paint + interval - now)
}

/// Hora de referencia del fotograma pintado.
///
/// Avanza en pasos fijos de `interval` para que el ritmo no se arrastre con los
/// despertares tempranos o tardíos; si se va más de un cuadro tarde (o el cuadro
/// se pintó antes por estar sucio), se resincroniza con `now`.
pub fn paint_mark(now: f64, last_paint: f64, interval: f64) -> f64 {
    let next = last_paint + interval;
    if now < next - FRAME_SLACK || now - next >= interval {
        now
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f64 = 1.0 / 60.0;

    #[test]
    fn frames_are_due_inside_the_clock_slack() {
        assert!(frame_due(1.0 + FRAME, 1.0, FRAME));
        assert!(frame_due(1.0 + FRAME - 0.001, 1.0, FRAME));
        assert!(!frame_due(1.0 + FRAME - 0.003, 1.0, FRAME));
        assert!(!frame_due(1.0, 1.0, FRAME));
    }

    #[test]
    fn paint_never_runs_hidden_or_dragging() {
        assert!(!paint_due(false, false, true, 10.0, 0.0, FRAME));
        assert!(!paint_due(true, true, true, 10.0, 0.0, FRAME));
        assert!(paint_due(true, false, true, 10.0, 0.0, FRAME));
        assert!(paint_due(true, false, false, 1.0 + FRAME, 1.0, FRAME));
        assert!(!paint_due(true, false, false, 1.0, 1.0, FRAME));
    }

    #[test]
    fn wait_delay_keeps_the_idle_loop_asleep() {
        assert_eq!(wait_delay(false, 0.0, FRAME, 100.0), 1.0 / 60.0);
        assert_eq!(wait_delay(true, 0.0, FRAME, 0.0), 1.0 / 60.0);
        let due = wait_delay(true, 1.0, FRAME, 1.0 + FRAME / 2.0);
        assert!((due - FRAME / 2.0).abs() < 1e-12);
        assert!(wait_delay(true, 0.0, FRAME, 10.0) < 0.0);
    }

    #[test]
    fn paint_mark_advances_in_fixed_steps() {
        let last = 1.0;
        let early = 1.0 + FRAME / 2.0;
        assert_eq!(paint_mark(early, last, FRAME), early);
        assert_eq!(paint_mark(1.0, last, FRAME), 1.0);
        let late = 1.0 + 3.0 * FRAME;
        assert_eq!(paint_mark(late, last, FRAME), late);
        assert_eq!(paint_mark(1.0 + FRAME - 0.001, last, FRAME), last + FRAME);
    }
}
