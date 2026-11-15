//! Paleta de «Marea» que `DropModel` interpola.
//!
//! Port de la tabla `_HEX`, `colors_for` y `CUSTOM_POSES` de
//! `src/ezagent/voice/hud/flow.py`. El dibujo del fluido es de F5-04; esta
//! tabla vive aquí porque el modelo la necesita en cada fotograma.

/// Cinco colores por pose: profundo, medio, brillante, acento y luz.
pub type Palette = [[f32; 3]; 5];

/// Poses que conservan el color personalizado; las de aviso tienen el suyo.
pub const CUSTOM_POSES: [&str; 7] = ["idle", "listen", "think", "speak", "done", "sleep", "mute"];

/// Estado visual (`UiState`) -> pose de la barra.
pub const POSE_OF: [(&str, &str); 8] = [
    ("off", "idle"),
    ("idle", "idle"),
    ("listen", "listen"),
    ("think", "think"),
    ("speak", "speak"),
    ("notice", "speak"),
    ("mute", "mute"),
    ("error", "error"),
];

/// Brillo de la forma por pose cuando no es el de fábrica.
pub const BRIGHT: [(&str, f64); 2] = [("mute", 0.9), ("sleep", 0.97)];

const IDLE: usize = 0;
const LISTEN: usize = 1;
const THINK: usize = 2;
const SPEAK: usize = 3;
const ASK: usize = 4;
const ERROR: usize = 5;
const MUTE: usize = 6;

/// `_HEX["light"]`, en RGB 0..255.
const LIGHT: [[[u8; 3]; 5]; 7] = [
    [
        [14, 111, 124],
        [31, 174, 159],
        [143, 227, 192],
        [255, 138, 101],
        [241, 255, 248],
    ],
    [
        [10, 98, 134],
        [22, 182, 194],
        [155, 239, 214],
        [255, 122, 89],
        [255, 255, 255],
    ],
    [
        [36, 71, 158],
        [31, 143, 176],
        [134, 210, 230],
        [155, 242, 255],
        [238, 248, 255],
    ],
    [
        [8, 112, 111],
        [20, 184, 152],
        [196, 238, 106],
        [255, 122, 89],
        [255, 255, 255],
    ],
    [
        [194, 102, 26],
        [238, 154, 52],
        [255, 212, 138],
        [255, 107, 74],
        [255, 244, 224],
    ],
    [
        [185, 38, 53],
        [226, 82, 90],
        [255, 175, 166],
        [255, 138, 101],
        [255, 230, 227],
    ],
    [
        [14, 111, 124],
        [31, 174, 159],
        [143, 227, 192],
        [255, 138, 101],
        [241, 255, 248],
    ],
];

/// `_HEX["dark"]`, en RGB 0..255.
const DARK: [[[u8; 3]; 5]; 7] = [
    [
        [5, 38, 46],
        [12, 102, 112],
        [41, 191, 169],
        [255, 122, 89],
        [140, 245, 218],
    ],
    [
        [4, 36, 55],
        [10, 115, 140],
        [44, 213, 203],
        [255, 138, 101],
        [176, 255, 242],
    ],
    [
        [10, 26, 62],
        [29, 63, 147],
        [47, 163, 196],
        [140, 240, 255],
        [202, 239, 255],
    ],
    [
        [3, 44, 42],
        [11, 133, 118],
        [120, 229, 154],
        [255, 138, 101],
        [218, 255, 180],
    ],
    [
        [55, 28, 5],
        [161, 87, 16],
        [240, 160, 56],
        [255, 107, 74],
        [255, 214, 142],
    ],
    [
        [54, 9, 15],
        [158, 34, 44],
        [226, 82, 90],
        [255, 138, 101],
        [255, 176, 168],
    ],
    [
        [5, 38, 46],
        [12, 102, 112],
        [41, 191, 169],
        [255, 122, 89],
        [140, 245, 218],
    ],
];

/// `(5, 3) float array: deep, mid, bright, accent, glow`.
pub fn colors_for(theme: &str, pose: &str) -> Palette {
    let table = if theme == "light" { &LIGHT } else { &DARK };
    let row = match pose {
        "idle" => IDLE,
        "listen" => LISTEN,
        "think" => THINK,
        "speak" => SPEAK,
        "ask" => ASK,
        "error" => ERROR,
        "mute" => MUTE,
        // Colgado («sleep») mantiene la calma; el gris queda solo para la pausa.
        "done" => SPEAK,
        _ => IDLE,
    };
    table[row].map(|color| color.map(|channel| channel as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_idle_palette_is_the_python_table() {
        assert_eq!(
            colors_for("light", "idle"),
            [
                [14.0, 111.0, 124.0],
                [31.0, 174.0, 159.0],
                [143.0, 227.0, 192.0],
                [255.0, 138.0, 101.0],
                [241.0, 255.0, 248.0],
            ]
        );
        assert_eq!(
            colors_for("dark", "idle"),
            [
                [5.0, 38.0, 46.0],
                [12.0, 102.0, 112.0],
                [41.0, 191.0, 169.0],
                [255.0, 122.0, 89.0],
                [140.0, 245.0, 218.0],
            ]
        );
    }

    #[test]
    fn done_speaks_and_unknown_poses_rest() {
        assert_eq!(colors_for("dark", "done"), colors_for("dark", "speak"));
        assert_eq!(colors_for("light", "sleep"), colors_for("light", "idle"));
        assert_eq!(colors_for("light", "nope"), colors_for("light", "idle"));
        assert_eq!(colors_for("other", "ask"), colors_for("dark", "ask"));
        assert_eq!(CUSTOM_POSES.len(), 7);
        assert!(CUSTOM_POSES.contains(&"sleep"));
    }

    #[test]
    fn the_dark_speak_accent_is_the_coral_filament() {
        assert_eq!(colors_for("dark", "speak")[3], [255.0, 138.0, 101.0]);
        assert_eq!(colors_for("dark", "idle")[3], [255.0, 122.0, 89.0]);
    }
}
