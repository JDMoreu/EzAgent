use std::fs;

use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ez-chat-media-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("carpeta temporal");
    dir
}

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 33];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    bytes[12..16].copy_from_slice(b"IHDR");
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    bytes
}

#[test]
fn absolute_file_urls_and_home_are_resolved() {
    let dir = temp_dir("resolve");
    let target = dir.join("a b.txt");
    fs::write(&target, "x").expect("archivo");
    assert_eq!(
        resolve_target(&target.to_string_lossy(), &[]),
        Some(target.clone())
    );
    let uri = format!(
        "file:///{}",
        target
            .to_string_lossy()
            .replace('\\', "/")
            .replace(' ', "%20")
    );
    assert_eq!(resolve_target(&uri, &[]), Some(target));
    assert!(resolve_target("~/algo.txt", &[]).is_some_and(|path| path.is_absolute()));
    assert_eq!(resolve_target("", &[]), None);
    assert_eq!(resolve_target("   ", &[]), None);
}

#[test]
fn relative_paths_only_resolve_inside_known_bases() {
    let dir = temp_dir("relative");
    fs::write(dir.join("x.txt"), "hola").expect("archivo");
    assert_eq!(
        resolve_target("x.txt", std::slice::from_ref(&dir)),
        Some(dir.join("x.txt"))
    );
    assert_eq!(resolve_target("x.txt", &[]), None);
    assert_eq!(
        resolve_target("x.txt", std::slice::from_ref(&dir.join("otra"))),
        None
    );
}

#[test]
fn is_within_handles_dotdot_and_siblings() {
    let root = temp_dir("within").join("ws");
    fs::create_dir_all(root.join("sub")).expect("carpeta");
    let inside = root.join("sub").join("a.txt");
    fs::write(&inside, "x").expect("archivo");
    assert!(is_within(&inside, std::slice::from_ref(&root)));
    assert!(!is_within(
        &root.join("sub").join("..").join("..").join("fuera.txt"),
        std::slice::from_ref(&root)
    ));
    assert!(!is_within(
        &root.parent().expect("padre").join("ws2").join("a.txt"),
        &[root]
    ));
    assert!(!is_within(&inside, &[]));
}

#[test]
fn file_info_describes_a_file_without_reading_it() {
    let dir = temp_dir("info");
    let doc = dir.join("Informe.DOCX");
    fs::write(&doc, vec![b'x'; 2048]).expect("archivo");
    let info = file_info(Some(&doc), None).expect("ficha");
    assert_eq!(
        (info.name.as_str(), info.ext.as_str(), info.size, info.kind),
        ("Informe.DOCX", ".docx", 2048, Kind::Office)
    );
    assert!(info.openable && info.allowed);
}

#[test]
fn missing_files_and_folders_have_no_card() {
    let dir = temp_dir("missing");
    assert!(file_info(Some(&dir.join("no-existe.pdf")), None).is_none());
    assert!(file_info(Some(&dir), None).is_none());
    assert!(file_info(None, None).is_none());
}

#[test]
fn executables_and_scripts_are_never_openable() {
    let dir = temp_dir("executables");
    for name in ["virus.exe", "run.bat", "x.cmd", "s.ps1", "a.lnk", "j.js"] {
        let path = dir.join(name);
        fs::write(&path, "x").expect("archivo");
        let info = file_info(Some(&path), None).expect("ficha");
        assert_eq!(info.kind, Kind::Other);
        assert!(!info.openable);
    }
}

#[test]
fn files_outside_the_allowed_roots_are_flagged() {
    let dir = temp_dir("roots");
    let allowed = dir.join("ok");
    fs::create_dir_all(&allowed).expect("carpeta");
    let inside = allowed.join("a.txt");
    let outside = dir.join("b.txt");
    fs::write(&inside, "x").expect("archivo");
    fs::write(&outside, "y").expect("archivo");
    assert!(
        file_info(Some(&inside), Some(std::slice::from_ref(&allowed)))
            .expect("ficha")
            .allowed
    );
    assert!(
        !file_info(Some(&outside), Some(std::slice::from_ref(&allowed)))
            .expect("ficha")
            .allowed
    );
    let forbidden = file_info(Some(&outside), Some(std::slice::from_ref(&allowed))).expect("ficha");
    assert!(!open_file(Some(&forbidden)));
    assert!(!reveal_file(Some(&forbidden)));
}

#[test]
fn human_size_uses_spanish_decimal_comma() {
    assert_eq!(human_size(0), "0 B");
    assert_eq!(human_size(12), "12 B");
    assert_eq!(human_size(820 * 1024), "820 KB");
    assert_eq!(human_size((1.4 * 1024.0 * 1024.0) as i64), "1,4 MB");
    assert_eq!(human_size(5 * 1024i64.pow(3)), "5,0 GB");
}

#[test]
fn a_local_image_is_validated_from_its_header() {
    let dir = temp_dir("image");
    let path = dir.join("a.png");
    fs::write(&path, png_bytes(40, 20)).expect("imagen");
    let info = file_info(Some(&path), None).expect("ficha");
    let image = load_image(Some(&info)).expect("imagen válida");
    assert_eq!((image.width, image.height, image.format), (40, 20, "png"));
    let corrupt = dir.join("rota.png");
    fs::write(&corrupt, b"no soy una imagen").expect("archivo");
    assert!(load_image(file_info(Some(&corrupt), None).as_ref()).is_none());
}

#[test]
fn oversized_and_forbidden_images_do_not_load() {
    let dir = temp_dir("image-limits");
    let huge = dir.join("grande.png");
    fs::write(&huge, png_bytes(7000, 7000)).expect("imagen");
    assert!(load_image(file_info(Some(&huge), None).as_ref()).is_none());
    let heavy = dir.join("pesa.png");
    let mut bytes = png_bytes(40, 20);
    bytes.resize(MAX_IMAGE_BYTES as usize + 1, 0);
    fs::write(&heavy, bytes).expect("imagen grande");
    assert!(load_image(file_info(Some(&heavy), None).as_ref()).is_none());
    let allowed = dir.join("ok");
    fs::create_dir_all(&allowed).expect("carpeta");
    let image = allowed.join("a.png");
    fs::write(&image, png_bytes(10, 10)).expect("imagen");
    let forbidden = file_info(Some(&image), Some(std::slice::from_ref(&dir.join("otra"))));
    assert!(load_image(forbidden.as_ref()).is_none());
    let text = dir.join("a.txt");
    fs::write(&text, "x").expect("archivo");
    assert!(load_image(file_info(Some(&text), None).as_ref()).is_none());
    assert!(load_image(None).is_none());
}

#[test]
fn a_gif_reports_only_its_first_frame_size() {
    let dir = temp_dir("gif");
    let path = dir.join("a.gif");
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&10_u16.to_le_bytes());
    bytes.extend_from_slice(&10_u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    fs::write(&path, bytes).expect("gif");
    let image = load_image(file_info(Some(&path), None).as_ref()).expect("gif válido");
    assert_eq!((image.width, image.height, image.format), (10, 10, "gif"));
}

#[test]
fn text_previews_truncate_strip_controls_and_mark_more() {
    let dir = temp_dir("text");
    let long = dir.join("notas.md");
    fs::write(&long, format!("# Notas\n{}", "línea larga ".repeat(400))).expect("archivo");
    let (text, more) = text_preview(file_info(Some(&long), None).as_ref()).expect("vista");
    assert!(text.starts_with("# Notas") && char_len(&text) <= PREVIEW_CHARS);
    assert!(more);

    let short = dir.join("a.txt");
    fs::write(&short, "corto").expect("archivo");
    assert_eq!(
        text_preview(file_info(Some(&short), None).as_ref()),
        Some(("corto".to_string(), false))
    );

    let control = dir.join("control.txt");
    fs::write(&control, b"hola\x00\x1b[31m rojo\x07\r\nfin").expect("archivo");
    let (text, _) = text_preview(file_info(Some(&control), None).as_ref()).expect("vista");
    assert!(!text.contains('\u{0}') && !text.contains('\u{1b}') && !text.contains('\u{7}'));
    assert_eq!(text.split('\n').next_back(), Some("fin"));
}

#[test]
fn a_csv_preview_is_a_markdown_table() {
    let dir = temp_dir("csv");
    let path = dir.join("ventas.csv");
    fs::write(&path, "mes,total\nenero,10\nfebrero,12\n").expect("archivo");
    let (text, more) = text_preview(file_info(Some(&path), None).as_ref()).expect("vista");
    let lines: Vec<&str> = text.split('\n').collect();
    assert_eq!(lines[0], "| mes | total |");
    assert_eq!(lines[1], "|---|---|");
    assert!(text.contains("| febrero | 12 |"));
    assert!(!more);
}

#[test]
fn a_long_csv_is_cut_to_the_first_rows() {
    let dir = temp_dir("csv-long");
    let path = dir.join("grande.csv");
    let body: String = (0..100).map(|i| format!("{i},{i}\n")).collect();
    fs::write(&path, format!("a,b\n{body}")).expect("archivo");
    let (text, more) = text_preview(file_info(Some(&path), None).as_ref()).expect("vista");
    assert!(text.split('\n').count() <= PREVIEW_ROWS + 3);
    assert!(more);
}

#[test]
fn previews_only_for_allowed_text_files() {
    let dir = temp_dir("preview-guard");
    let path = dir.join("a.txt");
    fs::write(&path, "x").expect("archivo");
    let forbidden = file_info(Some(&path), Some(std::slice::from_ref(&dir.join("otra"))));
    assert!(text_preview(forbidden.as_ref()).is_none());
    let png = dir.join("a.png");
    fs::write(&png, png_bytes(1, 1)).expect("imagen");
    assert!(text_preview(file_info(Some(&png), None).as_ref()).is_none());
    assert!(text_preview(None).is_none());
}
