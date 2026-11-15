//! Cuadros y estructura de `render_message` contra los PNG dorados.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ez_gfx::{Image, Rgba};
use ez_hud::paint::chat::render::{render_message, RenderOptions, Rendered};
use ez_hud::paint::chat::{Block, FileInfo, Hit, HitData, IconPainter, MediaKind};
use ez_hud::tokens::ColorMap;
use ez_parity::fixtures::FixtureCase;

use super::markdown;
use super::support::{a, b, f, i, line, num, o, s};

/// Ficha de un archivo del caso (`files` del oráculo).
struct Spec {
    kind: String,
    text: String,
    size: Option<[u64; 2]>,
    rgba: Option<[u8; 4]>,
    bytes: usize,
    mtime_wall: Vec<i64>,
}

fn media_kind(ext: &str) -> MediaKind {
    match ext {
        ".png" | ".jpg" | ".jpeg" | ".gif" | ".bmp" | ".webp" => MediaKind::Image,
        ".md" | ".markdown" | ".txt" | ".csv" | ".tsv" | ".json" | ".log" | ".toml" | ".yaml"
        | ".yml" => MediaKind::Text,
        ".pdf" | ".docx" | ".doc" | ".xlsx" | ".xls" | ".pptx" | ".ppt" | ".odt" | ".ods"
        | ".odp" | ".rtf" => MediaKind::Office,
        _ => MediaKind::Other,
    }
}

fn openable(ext: &str) -> bool {
    media_kind(ext) != MediaKind::Other
}

struct FixtureHost {
    files: BTreeMap<String, Spec>,
    image_size: Option<u64>,
}

impl FixtureHost {
    fn new(case: &FixtureCase) -> Self {
        let mut files = BTreeMap::new();
        if let Some(map) = case.input.get("files").and_then(|v| v.as_object()) {
            for (name, raw) in map {
                let kind = raw
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("other")
                    .to_string();
                let text = raw
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let size = raw
                    .get("size")
                    .and_then(|v| v.as_array())
                    .and_then(|values| Some([values.first()?.as_u64()?, values.get(1)?.as_u64()?]));
                let rgba = raw
                    .get("rgba")
                    .and_then(|v| v.as_array())
                    .and_then(|values| {
                        Some([
                            values.first()?.as_u64()? as u8,
                            values.get(1)?.as_u64()? as u8,
                            values.get(2)?.as_u64()? as u8,
                            values.get(3)?.as_u64()? as u8,
                        ])
                    });
                let bytes = raw
                    .get("hex")
                    .and_then(|v| v.as_str())
                    .map_or(0, |hex| hex.len() / 2);
                let mtime_wall = raw
                    .get("mtime_wall")
                    .and_then(|v| v.as_array())
                    .map(|items| items.iter().filter_map(|v| v.as_i64()).collect())
                    .unwrap_or_default();
                files.insert(
                    name.clone(),
                    Spec {
                        kind,
                        text,
                        size,
                        rgba,
                        bytes,
                        mtime_wall,
                    },
                );
            }
        }
        // El oráculo no guarda los bytes del PNG de prueba: el tamaño sale de
        // la ficha que el Python escribió en los `hits`.
        let image_size = case
            .output
            .get("hits")
            .and_then(|v| v.as_array())
            .and_then(|hits| {
                hits.iter().find_map(|hit| {
                    hit.get("data")
                        .and_then(|data| data.get("size"))
                        .and_then(|size| size.as_u64())
                })
            });
        Self { files, image_size }
    }

    fn spec(&self, path: &Path) -> Option<&Spec> {
        if let Some(name) = path.file_name() {
            if let Some(spec) = self.files.get(name.to_string_lossy().as_ref()) {
                return Some(spec);
            }
        }
        self.files
            .iter()
            .find(|(key, _)| path.ends_with(key.replace('/', "\\")))
            .map(|(_, spec)| spec)
    }
}

impl ez_hud::paint::chat::ChatHost for FixtureHost {
    fn parse(&self, text: &str) -> Vec<Block> {
        markdown::parse(text)
    }

    fn resolve_target(&self, target: &str, base_dirs: &[PathBuf]) -> Option<PathBuf> {
        let text = target.trim().trim_matches(['<', '>']);
        if text.is_empty() {
            return None;
        }
        // `Path("./x")` de Python no conserva el «.».
        let path: PathBuf = Path::new(text)
            .components()
            .filter(|part| !matches!(part, std::path::Component::CurDir))
            .collect();
        if path.is_absolute() {
            return Some(path);
        }
        for base in base_dirs {
            let candidate = base.join(&path);
            if self.spec(&candidate).is_some() {
                return Some(candidate);
            }
        }
        None
    }

    fn file_info(&self, path: Option<&Path>, roots: Option<&[PathBuf]>) -> Option<FileInfo> {
        let path = path?;
        let spec = self.spec(path)?;
        let name = path.file_name()?.to_string_lossy().into_owned();
        let ext = path
            .extension()
            .map(|ext| format!(".{}", ext.to_string_lossy().to_lowercase()))
            .unwrap_or_default();
        let size = match spec.kind.as_str() {
            "text" => spec.text.len() as u64,
            "image" => self.image_size.unwrap_or(0),
            _ => spec.bytes as u64,
        };
        let allowed = roots.is_none_or(|roots| roots.iter().any(|root| path.starts_with(root)));
        let mtime = super::support::epoch_from_wall(&spec.mtime_wall);
        Some(FileInfo {
            path: path.to_path_buf(),
            name,
            ext: ext.clone(),
            size,
            mtime,
            kind: media_kind(&ext),
            allowed,
            openable: openable(&ext),
        })
    }

    fn load_image(&self, info: &FileInfo) -> Option<Image> {
        let spec = self.spec(&info.path)?;
        if spec.kind != "image" || !info.allowed {
            return None;
        }
        let [width, height] = spec.size?;
        let rgba = spec.rgba?;
        let pixels = vec![rgba; (width * height) as usize].concat();
        Image::from_rgba(width as u32, height as u32, pixels).ok()
    }

    fn text_preview(&self, info: &FileInfo) -> Option<(String, bool)> {
        let spec = self.spec(&info.path)?;
        if spec.kind != "text" || !info.allowed {
            return None;
        }
        Some((spec.text.trim_end().to_string(), false))
    }
}

struct Icons;

impl IconPainter for Icons {
    fn draw(
        &self,
        tile: &mut [f32],
        side: u32,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
        color: Rgba,
        scale: f64,
    ) {
        let _ = ez_gfx::icons::draw_icon(
            tile,
            side as usize,
            side as usize,
            name,
            cx,
            cy,
            size,
            ez_gfx::icons::Color::Rgba(color.r, color.g, color.b, color.a),
            scale,
            1.0,
        );
    }
}

fn paths(case: &FixtureCase, key: &str, root: &str) -> Option<Vec<PathBuf>> {
    case.input
        .get(key)
        .filter(|value| !value.is_null())
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .map(|value| {
                    PathBuf::from(
                        value
                            .as_str()
                            .unwrap_or("")
                            .replace("<tmp>", root)
                            .replace('/', "\\"),
                    )
                })
                .collect()
        })
}

pub(super) fn render(case: &FixtureCase, root: &str) {
    let custom = case.input.get("custom").and_then(|v| v.as_object());
    if let Some(custom) = custom {
        let primary = custom
            .get("primary")
            .and_then(|v| v.as_str())
            .unwrap_or("#000000");
        let background = custom
            .get("background")
            .and_then(|v| v.as_str())
            .unwrap_or("#000000");
        ez_hud::tokens::set_themed(ez_hud::theme::derive(primary, background).as_ref());
    } else {
        ez_hud::tokens::set_themed(None);
    }
    let theme = line(case, "theme");
    let pal: ColorMap = ez_hud::tokens::palette(&theme).expect("la paleta del caso");
    let host = FixtureHost::new(case);
    let options = RenderOptions {
        scale: num(case, "scale"),
        roots: paths(case, "roots", root),
        base_dirs: paths(case, "base_dirs", root).unwrap_or_default(),
        icons: Some(&Icons),
    };
    let rendered = render_message(
        &host,
        &line(case, "text"),
        &pal,
        num(case, "width"),
        &options,
    );
    compare_structure(case, &rendered, root);
    compare_png(case, &rendered);
    ez_hud::tokens::set_themed(None);
}

fn compare_structure(case: &FixtureCase, rendered: &Rendered, root: &str) {
    let output = &case.output;
    let height = ez_core::pyjson::dumps(output.get("height").expect("height"));
    let image_px = ez_core::pyjson::dumps(output.get("image_px").expect("image_px"));
    let plain = ez_core::pyjson::dumps(output.get("plain").expect("plain"));
    let hits = ez_core::pyjson::dumps(output.get("hits").expect("hits"));
    let expected = format!(
        "{{\"height\": {height}, \"image_px\": {image_px}, \"plain\": {plain}, \"hits\": {hits}}}"
    );
    let produced = rendered_summary(rendered);
    super::support::same(&case.case, &expected, &produced, Some(root));
}

fn compare_png(case: &FixtureCase, rendered: &Rendered) {
    let Some(png) = case.output.get("png").and_then(|v| v.as_str()) else {
        return;
    };
    let path = Path::new("../../../parity/golden").join(png);
    let expected = ez_parity::image::decode(&path).expect("PNG dorado");
    let raw = rendered.image.as_raw();
    let produced = image::RgbaImage::from_raw(
        rendered.image.width(),
        rendered.image.height(),
        raw.to_vec(),
    )
    .expect("búfer RGBA del mensaje");
    let delta = ez_parity::image::compare(&produced, &expected);
    let thresholds =
        ez_parity::image::Thresholds::load(Path::new("../../../parity/thresholds.toml"))
            .expect("umbrales")
            .pixels
            .text;
    if !ez_parity::image::within(&delta, &thresholds) {
        super::png_failures()
            .lock()
            .expect("sin envenenar")
            .push(format!(
                "{} ({png}): max={} p995={} p999={} ssim={}",
                case.case, delta.max, delta.p995, delta.p999, delta.ssim
            ));
    }
}

fn rendered_summary(rendered: &Rendered) -> String {
    let hits: Vec<String> = rendered.hits.iter().map(hit_json).collect();
    o(&[
        ("height", f(rendered.height)),
        (
            "image_px",
            a(&[
                i(i64::from(rendered.image.width())),
                i(i64::from(rendered.image.height())),
            ]),
        ),
        ("plain", s(&rendered.plain)),
        ("hits", a(&hits)),
    ])
}

fn hit_json(hit: &Hit) -> String {
    let data = match &hit.data {
        None => "null".into(),
        Some(HitData::Text(text)) => s(text),
        Some(HitData::File(file)) => file_json(file),
    };
    o(&[
        ("kind", s(hit.kind)),
        (
            "rect",
            a(&[
                f(hit.rect.x0),
                f(hit.rect.y0),
                f(hit.rect.x1),
                f(hit.rect.y1),
            ]),
        ),
        ("data", data),
    ])
}

fn file_json(file: &FileInfo) -> String {
    let wall = wall_of(file.mtime);
    o(&[
        ("name", s(&file.name)),
        ("ext", s(&file.ext)),
        ("size", i(file.size as i64)),
        ("kind", s(kind_name(file.kind))),
        ("allowed", b(file.allowed)),
        ("openable", b(file.openable)),
        ("path", s(&file.path.to_string_lossy())),
        (
            "mtime_wall",
            a(&wall.iter().map(|value| i(*value)).collect::<Vec<_>>()),
        ),
    ])
}

fn kind_name(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "image",
        MediaKind::Text => "text",
        MediaKind::Office => "office",
        MediaKind::Other => "other",
    }
}

fn wall_of(t: f64) -> Vec<i64> {
    let iso = ez_core::pytime::local_iso_seconds(t.trunc()).unwrap_or_default();
    iso.split(['-', 'T', ':'])
        .filter_map(|part| part.parse().ok())
        .take(6)
        .collect()
}
