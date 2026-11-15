//! Issue #7: FBX staging copies only the images the FBX names, read from the
//! binary FBX the way Blender's importer reads it.
use deadlock_core::CancellationToken;
use deadlock_preparation::{fbx_images::image_names, source};
use std::{collections::BTreeSet, fs, io::Cursor, path::Path};

enum P {
    S(&'static [u8]),
    R(&'static [u8]),
    /// A scalar: its type byte and size.
    N(u8, usize),
    /// An array: type byte, element count, encoding, stored bytes.
    A(u8, u32, u32, usize),
}
struct N {
    name: &'static str,
    props: Vec<P>,
    children: Vec<N>,
}
fn n(name: &'static str, props: Vec<P>, children: Vec<N>) -> N {
    N { name, props, children }
}
fn word(out: &mut Vec<u8>, wide: bool, value: u64) {
    if wide {
        out.extend(value.to_le_bytes());
    } else {
        out.extend((value as u32).to_le_bytes());
    }
}
/// One node record written at file offset `at`, as Blender's `encode_bin.py` lays it out.
fn encode(node: &N, wide: bool, at: usize) -> Vec<u8> {
    let mut props = vec![];
    for p in &node.props {
        match p {
            P::S(text) | P::R(text) => {
                props.push(if matches!(p, P::S(_)) { b'S' } else { b'R' });
                props.extend((text.len() as u32).to_le_bytes());
                props.extend(*text);
            }
            P::N(kind, size) => {
                props.push(*kind);
                props.extend(vec![7; *size]);
            }
            P::A(kind, count, encoding, stored) => {
                props.push(*kind);
                props.extend(count.to_le_bytes());
                props.extend(encoding.to_le_bytes());
                props.extend((*stored as u32).to_le_bytes());
                props.extend(vec![9; *stored]);
            }
        }
    }
    let sentinel = if wide { 25 } else { 13 };
    let header = if wide { 25 } else { 13 } + node.name.len();
    let mut children = vec![];
    for child in &node.children {
        let start = at + header + props.len() + children.len();
        children.extend(encode(child, wide, start));
    }
    if !node.children.is_empty() {
        children.extend(vec![0; sentinel]);
    }
    let total = header + props.len() + children.len();
    let mut out = vec![];
    word(&mut out, wide, (at + total) as u64);
    word(&mut out, wide, node.props.len() as u64);
    word(&mut out, wide, props.len() as u64);
    out.push(node.name.len() as u8);
    out.extend(node.name.as_bytes());
    out.extend(props);
    out.extend(children);
    out
}
fn document(version: u32, nodes: &[N]) -> Vec<u8> {
    let wide = version >= 7500;
    let mut out = b"Kaydara FBX Binary  \x00\x1a\x00".to_vec();
    out.extend(version.to_le_bytes());
    for node in nodes {
        let bytes = encode(node, wide, out.len());
        out.extend(bytes);
    }
    out.extend(vec![0; if wide { 25 } else { 13 }]);
    out.extend(b"footer bytes Blender ignores");
    out
}
/// A character with a texture, a video with embedded data and a mesh, plus
/// every property type Blender's reader accepts.
fn character(texture: &'static [u8]) -> Vec<N> {
    vec![
        n("FBXHeaderExtension", vec![], vec![
            n("FBXVersion", vec![P::N(b'I', 4)], vec![]),
            n("Flags", vec![P::N(b'Z', 1), P::N(b'B', 1), P::N(b'C', 1), P::N(b'Y', 2), P::N(b'F', 4), P::N(b'D', 8), P::N(b'L', 8)], vec![]),
        ]),
        n("Definitions", vec![], vec![n("ObjectType", vec![P::S(b"Texture")], vec![])]),
        n("Objects", vec![], vec![
            n("Geometry", vec![P::N(b'L', 8), P::S(b"Body\x00\x01Geometry"), P::S(b"Mesh")], vec![
                n("Vertices", vec![P::A(b'd', 3000, 0, 24000)], vec![]),
                n("PolygonVertexIndex", vec![P::A(b'i', 4000, 1, 321)], vec![]),
                n("Flags", vec![P::A(b'b', 2, 0, 2), P::A(b'c', 1, 0, 1), P::A(b'l', 1, 0, 8), P::A(b'f', 1, 0, 4)], vec![]),
            ]),
            n("Texture", vec![P::N(b'L', 8), P::S(b"Diffuse\x00\x01Texture"), P::S(b"")], vec![
                n("Type", vec![P::S(b"TextureVideoClip")], vec![]),
                n("FileName", vec![P::S(br"C:\Artist\tex\Body_D.PNG")], vec![]),
                n("RelativeFilename", vec![P::S(texture)], vec![]),
            ]),
            n("Video", vec![P::N(b'L', 8), P::S("Лицо\x00\x01Video".as_bytes()), P::S(b"Clip")], vec![
                n("Filename", vec![P::S(b"/home/artist/Face.tga")], vec![]),
                n("Content", vec![P::R(b"embedded pixels")], vec![]),
            ]),
        ]),
    ]
}
fn names(bytes: &[u8]) -> Option<BTreeSet<String>> {
    image_names(Cursor::new(bytes), bytes.len() as u64, &CancellationToken::new()).unwrap()
}
fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn texture_and_video_paths_are_read_from_32_and_64_bit_layouts() {
    let expected = set(&["body_d.png", "diffuse", "face.tga", "лицо"]);
    for version in [7400, 7500, 7700] {
        let bytes = document(version, &character(br"tex\Body_D.png"));
        assert_eq!(names(&bytes), Some(expected.clone()), "{version}");
    }
    // An FBX without textures names no image.
    let bare = document(7400, &[n("Objects", vec![], vec![n("Model", vec![P::N(b'L', 8), P::S(b"Body\x00\x01Model"), P::S(b"Mesh")], vec![])])]);
    assert_eq!(names(&bare), Some(BTreeSet::new()));
}

#[test]
fn malformed_or_other_files_are_not_read() {
    let bytes = document(7400, &character(b"Body_D.png"));
    let null_record = bytes.len() - 13 - b"footer bytes Blender ignores".len();
    // Every cut before the closing null record, never a panic or a partial set.
    for cut in 0..null_record + 13 {
        assert_eq!(names(&bytes[..cut]), None, "{cut}");
    }
    assert!(names(&bytes[..null_record + 13]).is_some());
    assert_eq!(names(b"; FBX 7.4.0 project file\nObjects: {}\n"), None);
    assert_eq!(names(b""), None);
    let corrupt = |at: usize, value: u8| {
        let mut copy = bytes.clone();
        copy[at] = value;
        names(&copy)
    };
    let first = 27;
    // An end offset past the file, before the record, a huge property count,
    // an unknown property type and a sentinel that is not zero.
    assert_eq!(corrupt(first + 3, 0x7f), None);
    assert_eq!(corrupt(first, 1), None);
    assert_eq!(corrupt(first + 7, 0x7f), None);
    let flags = bytes.windows(5).position(|w| w == b"Flags").unwrap() + 5;
    assert_eq!(corrupt(flags, b'X'), None);
    let children_end = bytes.windows(10).position(|w| w == b"Definition").unwrap() - 13 - 1;
    assert_eq!(corrupt(children_end, 1), None);
    // A length claimed beyond the file is refused before any allocation.
    let mut huge = bytes.clone();
    let texture = huge.windows(16).position(|w| w == b"RelativeFilename").unwrap() + 17;
    huge[texture..texture + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(names(&huge), None);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(image_names(Cursor::new(&bytes), bytes.len() as u64, &cancel).is_err());
}

// 206 characters but 261 UTF-8 bytes, past the 255-byte name limit of Linux file systems.
#[cfg(windows)]
const WALLPAPER: &str = "#фон #небо #город #море #лес #поле #река #гора #сад #дом #мост #парк #луна #снег #да #wallpaper #sky #city #sea #forest #field #river #hill #garden #house #bridge #park #moon #snow #example #long #file .jpg";

#[cfg(windows)]
fn staged(work: &Path) -> BTreeSet<String> {
    let manifest: Vec<source::ManifestEntry> =
        serde_json::from_slice(&fs::read(work.join("source-manifest.json")).unwrap()).unwrap();
    for entry in &manifest {
        assert_eq!(deadlock_preparation::digest(&work.join(&entry.relative)).unwrap(), entry.sha256);
    }
    manifest.into_iter().map(|e| e.relative).collect()
}

#[cfg(windows)]
#[test]
fn fbx_staging_copies_only_the_images_the_fbx_names() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    for sub in ["textures", "old", "Programs"] {
        fs::create_dir_all(folder.join(sub)).unwrap();
    }
    fs::write(folder.join("textures/body_d.png"), b"body").unwrap();
    fs::write(folder.join("old/BODY_D.PNG"), b"older body").unwrap();
    fs::write(folder.join("unrelated.png"), b"other").unwrap();
    fs::write(folder.join("Programs").join(WALLPAPER), b"wallpaper").unwrap();
    let fbx = folder.join("hero.fbx");
    fs::write(&fbx, document(7400, &character(b"textures/Body_D.png"))).unwrap();
    let work = temp.path().join("work");
    let local = source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    assert_eq!(local, work.join("hero.fbx"));
    // Both files Blender's image search could pick, nothing else.
    assert_eq!(staged(&work), set(&["hero.fbx", "old/BODY_D.PNG", "textures/body_d.png"]));
    assert!(!work.join("Programs").exists());
    // An FBX that names no image stages only itself.
    fs::write(&fbx, document(7500, &[])).unwrap();
    let work = temp.path().join("bare");
    source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    assert_eq!(staged(&work), set(&["hero.fbx"]));
}

#[cfg(windows)]
#[test]
fn unreadable_fbx_stages_every_image_blender_can_open() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(folder.join("Programs")).unwrap();
    fs::write(folder.join("body.png"), b"body").unwrap();
    fs::write(folder.join("Programs").join(WALLPAPER), b"wallpaper").unwrap();
    let fbx = folder.join("hero.fbx");
    fs::write(&fbx, b"; FBX 7.4.0 project file\n").unwrap();
    // A workspace deep enough for the copy to pass 260 characters on any PC.
    let work = temp.path().join("w".repeat(60));
    source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    assert!(source::too_long_for_blender(&work.join("Programs").join(WALLPAPER)).is_some());
    // Blender could not open it, so it is left out.
    assert_eq!(staged(&work), set(&["body.png", "hero.fbx"]));
}

#[cfg(windows)]
#[test]
fn a_named_image_too_long_for_blender_stops_staging_with_the_way_out() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(folder.join("Programs")).unwrap();
    fs::write(folder.join("Programs").join(WALLPAPER), b"wallpaper").unwrap();
    let fbx = folder.join("hero.fbx");
    let name: &'static str = Box::leak(format!("Programs/{WALLPAPER}").into_boxed_str());
    fs::write(&fbx, document(7400, &character(name.as_bytes()))).unwrap();
    // Deep enough for the copy to pass 260 characters on any PC.
    let work = temp.path().join("w".repeat(60));
    let error = source::stage_source(fbx.to_str().unwrap(), &work, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains(WALLPAPER) && error.contains("too long for Blender"), "{error}");
    assert!(error.contains("shorter name") && error.contains("Workspace folder in Settings"), "{error}");
    // Nothing is copied before the check.
    assert!(!work.join("hero.fbx").exists());
}

#[test]
fn blender_path_limit_is_260_utf16_units() {
    assert_eq!(source::too_long_for_blender(Path::new(&"a".repeat(259))), None);
    assert_eq!(source::too_long_for_blender(Path::new(&"a".repeat(260))), Some(260));
    assert_eq!(source::too_long_for_blender(Path::new(&"я".repeat(260))), Some(260));
}

#[test]
fn a_workspace_inside_the_model_folder_is_not_searched_again() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("Models");
    fs::create_dir_all(folder.join("tex")).unwrap();
    fs::write(folder.join("tex/body.png"), b"body").unwrap();
    let fbx = folder.join("hero.fbx");
    fs::write(&fbx, document(7400, &character(b"body.png"))).unwrap();
    // A first preparation, then a second one beside it in the same workspace.
    let workspace = folder.join("workspace").join("project");
    source::stage_source(fbx.to_str().unwrap(), &workspace.join("run1"), None).unwrap();
    assert!(workspace.join("run1/tex/body.png").is_file());
    let second = workspace.join("run2");
    source::stage_source(fbx.to_str().unwrap(), &second, None).unwrap();
    let manifest: Vec<source::ManifestEntry> =
        serde_json::from_slice(&fs::read(second.join("source-manifest.json")).unwrap()).unwrap();
    let staged: BTreeSet<_> = manifest.into_iter().map(|e| e.relative).collect();
    assert_eq!(staged, set(&["hero.fbx", "tex/body.png"]));
}
