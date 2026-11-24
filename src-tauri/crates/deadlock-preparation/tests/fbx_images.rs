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
    /// An object id.
    L(i64),
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
            P::L(id) => {
                props.push(b'L');
                props.extend(id.to_le_bytes());
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
fn a_named_image_too_long_for_blender_is_staged_under_a_short_name() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(folder.join("Programs")).unwrap();
    fs::write(folder.join("Programs").join(WALLPAPER), b"wallpaper").unwrap();
    let fbx = folder.join("hero.fbx");
    let name: &'static str = Box::leak(format!("Programs/{WALLPAPER}").into_boxed_str());
    fs::write(&fbx, document(7400, &character(name.as_bytes()))).unwrap();
    // Deep enough for the copy to pass 260 characters on any PC, even
    // directly in the staging folder under its own name.
    let work = temp.path().join("w".repeat(60));
    assert!(source::too_long_for_blender(&work.join(WALLPAPER)).is_some());
    source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    let entries = manifest(&work);
    let alias = short_entry(&entries, &format!("Programs/{WALLPAPER}"));
    assert_eq!(fs::read(work.join(&alias)).unwrap(), b"wallpaper");
    assert!(!work.join("Programs").exists());
}

// Models whose textures have long names: an image whose staged copy would be
// too long for Blender is staged directly in the staging folder, under its
// own name when that is enough and no other staged file has it, else under a
// short name; the manifest's `origin` says where it came from.

#[cfg(windows)]
fn manifest(work: &Path) -> Vec<source::ManifestEntry> {
    let text = fs::read_to_string(work.join("source-manifest.json")).unwrap();
    let entries: Vec<source::ManifestEntry> = serde_json::from_str(&text).unwrap();
    for entry in &entries {
        assert_eq!(deadlock_preparation::digest(&work.join(&entry.relative)).unwrap(), entry.sha256);
    }
    entries
}

/// The staged name of the image from `origin`, which must be a short name:
/// 16 hex digits and the image's extension.
#[cfg(windows)]
fn short_entry(entries: &[source::ManifestEntry], origin: &str) -> String {
    let entry = entries.iter().find(|e| e.origin.as_deref() == Some(origin)).unwrap_or_else(|| panic!("{origin} in {entries:?}"));
    let (stem, extension) = entry.relative.rsplit_once('.').unwrap();
    assert!(stem.len() == 16 && stem.bytes().all(|b| b.is_ascii_hexdigit()), "{}", entry.relative);
    assert_eq!(Some(extension), origin.rsplit_once('.').map(|(_, e)| e));
    entry.relative.clone()
}

/// A folder below `base` whose path is `units` characters long.
#[cfg(windows)]
fn deep(base: &Path, units: usize) -> std::path::PathBuf {
    let mut length = base.as_os_str().encode_wide_len();
    assert!(units > length + 2, "the temporary folder is too long for this test");
    let mut path = base.to_path_buf();
    // Folder names of at most 100 characters, each after a separator.
    while length < units {
        let room = units - length - 1;
        let part = if room <= 100 { room } else if room == 101 { 98 } else { 100 };
        path.push("d".repeat(part));
        length += 1 + part;
    }
    assert_eq!(path.as_os_str().encode_wide_len(), units);
    path
}

#[cfg(windows)]
trait WideLen {
    fn encode_wide_len(&self) -> usize;
}
#[cfg(windows)]
impl WideLen for std::ffi::OsStr {
    fn encode_wide_len(&self) -> usize {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().count()
    }
}

/// An FBX beside `<model>.fbm` holding its texture, as exporters write it.
#[cfg(windows)]
fn fbm_model(folder: &Path, fbm: &str, texture: &str) -> std::path::PathBuf {
    fs::create_dir_all(folder.join(fbm)).unwrap();
    fs::write(folder.join(fbm).join(texture), texture.as_bytes()).unwrap();
    let fbx = folder.join("hero.fbx");
    let relative: &'static str = Box::leak(format!("{fbm}\\{texture}").into_boxed_str());
    fs::write(&fbx, document(7500, &character(relative.as_bytes()))).unwrap();
    fbx
}

#[cfg(windows)]
const FBM: &str = "hero_convert_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcd.fbm";
#[cfg(windows)]
const TEXTURE: &str = "hero_convert_0123456789abcdef_texture_basecolor_0123456789.png";

#[cfg(windows)]
#[test]
fn an_ordinary_fbm_model_stages_exactly_as_before() {
    let temp = tempfile::tempdir().unwrap();
    let fbx = fbm_model(&temp.path().join("model"), "hero.fbm", "Body_D.png");
    let work = temp.path().join("work");
    source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    // The manifest has only the keys it always had, and the same paths.
    let raw: Vec<serde_json::Map<String, serde_json::Value>> =
        serde_json::from_slice(&fs::read(work.join("source-manifest.json")).unwrap()).unwrap();
    let relatives: Vec<_> = raw.iter().map(|e| e["relative"].as_str().unwrap().to_owned()).collect();
    assert_eq!(relatives, ["hero.fbm/Body_D.png", "hero.fbx"]);
    for entry in &raw {
        assert_eq!(entry.keys().map(String::as_str).collect::<Vec<_>>(), ["relative", "sha256"]);
    }
    assert!(manifest(&work).iter().all(|e| e.origin.is_none()));
    // Byte for byte the manifest staging always wrote.
    let sha = |path: &Path| deadlock_preparation::digest(path).unwrap();
    let expected = format!(
        "[\n  {{\n    \"relative\": \"hero.fbm/Body_D.png\",\n    \"sha256\": \"{}\"\n  }},\n  {{\n    \"relative\": \"hero.fbx\",\n    \"sha256\": \"{}\"\n  }}\n]",
        sha(&work.join("hero.fbm/Body_D.png")),
        sha(&fbx)
    );
    let text = fs::read_to_string(work.join("source-manifest.json")).unwrap();
    assert_eq!(text.trim_end(), expected);
}

/// A manifest written before shorter copies existed (no `origin`, no
/// `found`) still gives the same proof, and it still verifies.
#[test]
fn a_manifest_from_before_shorter_copies_still_proves_and_verifies() {
    let temp = tempfile::tempdir().unwrap();
    let model = temp.path().join("model");
    fs::create_dir_all(model.join("hero.fbm")).unwrap();
    fs::write(model.join("hero.fbx"), b"fbx").unwrap();
    fs::write(model.join("hero.fbm/Body_D.png"), b"body").unwrap();
    let staged = temp.path().join("staged");
    fs::create_dir_all(&staged).unwrap();
    let sha = |path: &Path| deadlock_preparation::digest(path).unwrap();
    let old = format!(
        "[\n  {{\n    \"relative\": \"hero.fbm/Body_D.png\",\n    \"sha256\": \"{}\"\n  }},\n  {{\n    \"relative\": \"hero.fbx\",\n    \"sha256\": \"{}\"\n  }}\n]",
        sha(&model.join("hero.fbm/Body_D.png")),
        sha(&model.join("hero.fbx"))
    );
    fs::write(staged.join("source-manifest.json"), &old).unwrap();
    let proof = deadlock_preparation::provenance::source_proof(&model.join("hero.fbx"), &staged.join("hero.fbx")).unwrap();
    let relatives: Vec<_> = proof.iter().map(|p| p.relative.as_str()).collect();
    assert_eq!(relatives, ["hero.fbm/Body_D.png", ""]);
    let source = model.join("hero.fbx");
    let source = source.to_str().unwrap();
    deadlock_preparation::provenance::verify_files(source, source, &proof).unwrap();
    // Old entries read back with no origin and serialise to the same text.
    let entries: Vec<source::ManifestEntry> = serde_json::from_str(&old).unwrap();
    assert!(entries.iter().all(|e| e.origin.is_none() && e.found.is_none()));
    assert_eq!(serde_json::to_string_pretty(&entries).unwrap(), old);
}

#[cfg(windows)]
#[test]
fn a_texture_too_long_in_its_folder_is_staged_beside_the_model_under_its_own_name() {
    let temp = tempfile::tempdir().unwrap();
    let fbx = fbm_model(&temp.path().join("model"), FBM, TEXTURE);
    // Too long below the .fbm folder, short enough beside the model.
    let work = deep(temp.path(), 150);
    assert!(source::too_long_for_blender(&work.join(FBM).join(TEXTURE)).is_some());
    assert!(source::too_long_for_blender(&work.join(TEXTURE)).is_none());
    let local = source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    let entries = manifest(&work);
    let origin = format!("{FBM}/{TEXTURE}");
    let entry = entries.iter().find(|e| e.relative == TEXTURE).unwrap();
    assert_eq!(entry.origin.as_deref(), Some(origin.as_str()));
    assert!(!work.join(FBM).exists());
    // The proof names the user's file at its own place, and it verifies.
    let proof = deadlock_preparation::provenance::source_proof(&fbx, &local).unwrap();
    let relatives: BTreeSet<_> = proof.iter().map(|p| p.relative.clone()).collect();
    assert_eq!(relatives, set(&["", &origin]));
    let source = fbx.to_str().unwrap();
    deadlock_preparation::provenance::verify_files(source, source, &proof).unwrap();
    fs::write(fbx.parent().unwrap().join(FBM).join(TEXTURE), b"changed").unwrap();
    let error = deadlock_preparation::provenance::verify_files(source, source, &proof).unwrap_err().to_string();
    assert!(error.contains("Changed or missing source input") && error.contains(TEXTURE), "{error}");
}

#[cfg(windows)]
#[test]
fn a_texture_name_too_long_on_its_own_gets_a_short_name_again_on_prepare_again() {
    let temp = tempfile::tempdir().unwrap();
    let long = format!("{}.png", "texture_".repeat(20));
    let fbx = fbm_model(&temp.path().join("model"), FBM, &long);
    let work = deep(temp.path(), 120);
    assert!(source::too_long_for_blender(&work.join(&long)).is_some());
    let local = source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    let first = manifest(&work);
    let origin = format!("{FBM}/{long}");
    let alias = short_entry(&first, &origin);
    assert_eq!(fs::read(work.join(&alias)).unwrap(), long.as_bytes());
    let proof = deadlock_preparation::provenance::source_proof(&fbx, &local).unwrap();
    assert!(proof.iter().any(|p| p.relative == origin));
    deadlock_preparation::provenance::verify_files(fbx.to_str().unwrap(), fbx.to_str().unwrap(), &proof).unwrap();
    // Importing the user's file again stages into a new folder of the same
    // depth with the same names.
    let again = deep(&temp.path().join("x"), 120);
    source::stage_source(fbx.to_str().unwrap(), &again, None).unwrap();
    assert_eq!(manifest(&again), first);
    // Prepare again imports the saved workspace copy: its short copy still
    // stands for the texture the model names, under the same short name, and
    // its proof points at the copy in that folder.
    let third = deep(&temp.path().join("z"), 120);
    let restaged = source::stage_source(local.to_str().unwrap(), &third, None).unwrap();
    let entries = manifest(&third);
    let entry = entries.iter().find(|e| e.relative == alias).unwrap();
    assert_eq!((entry.origin.as_deref(), entry.found.as_deref()), (Some(origin.as_str()), Some(alias.as_str())));
    assert_eq!(fs::read(third.join(&alias)).unwrap(), long.as_bytes());
    let proof = deadlock_preparation::provenance::source_proof(&local, &restaged).unwrap();
    deadlock_preparation::provenance::verify_files(local.to_str().unwrap(), local.to_str().unwrap(), &proof).unwrap();
    // In a shorter workspace the copy takes the shortest place that fits for
    // the name the model asks for: here its own name beside the model.
    let short = temp.path().join("s");
    source::stage_source(local.to_str().unwrap(), &short, None).unwrap();
    let entry = manifest(&short).into_iter().find(|e| e.found.as_deref() == Some(alias.as_str())).unwrap();
    assert_eq!((entry.relative.as_str(), entry.origin.as_deref()), (long.as_str(), Some(origin.as_str())));
}

#[cfg(windows)]
#[test]
fn prepare_again_finds_a_texture_staged_beside_the_model() {
    let temp = tempfile::tempdir().unwrap();
    let fbx = fbm_model(&temp.path().join("model"), FBM, TEXTURE);
    let work = deep(temp.path(), 150);
    let local = source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    let first = manifest(&work);
    let again = deep(&temp.path().join("x"), 150);
    source::stage_source(local.to_str().unwrap(), &again, None).unwrap();
    let mut entries = manifest(&again);
    let entry = entries.iter_mut().find(|e| e.relative == TEXTURE).unwrap();
    assert_eq!(entry.found.take().as_deref(), Some(TEXTURE));
    assert_eq!(entries, first);
}

#[cfg(windows)]
#[test]
fn a_long_texture_sharing_its_name_with_another_never_replaces_it() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    // The FBX names the texture in the long folder; another one of the same
    // name sits in a short folder, where Blender's search would also find it.
    let fbx = fbm_model(&folder, FBM, "Body_D.png");
    fs::create_dir_all(folder.join("old")).unwrap();
    fs::write(folder.join("old/BODY_D.PNG"), b"older body").unwrap();
    let work = deep(temp.path(), 175);
    assert!(source::too_long_for_blender(&work.join(FBM).join("Body_D.png")).is_some());
    source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap();
    let entries = manifest(&work);
    let alias = short_entry(&entries, &format!("{FBM}/Body_D.png"));
    assert_eq!(fs::read(work.join(&alias)).unwrap(), b"Body_D.png");
    // The other keeps its place and its bytes, and nothing lands at the top under that name.
    let old = entries.iter().find(|e| e.relative == "old/BODY_D.PNG").unwrap();
    assert!(old.origin.is_none());
    assert_eq!(fs::read(work.join("old/BODY_D.PNG")).unwrap(), b"older body");
    assert!(!work.join("Body_D.png").exists());
}

#[cfg(windows)]
#[test]
fn only_a_workspace_too_long_for_any_short_name_or_the_model_stops_staging() {
    let temp = tempfile::tempdir().unwrap();
    let fbx = fbm_model(&temp.path().join("model"), FBM, TEXTURE);
    // hero.fbx fits (250 + 9), a 16-digit name with .png (250 + 21) does not.
    let work = deep(temp.path(), 250);
    let error = source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap_err().to_string();
    assert!(error.contains(TEXTURE) && error.contains("even under a short name: its path there would be 271 characters"), "{error}");
    assert!(error.contains("Workspace folder in Settings") && !error.contains("shorter name"), "{error}");
    assert!(!work.join("hero.fbx").exists());
    // The model's own copy too long.
    let work = deep(temp.path(), 252);
    let error = source::stage_source(fbx.to_str().unwrap(), &work, None).unwrap_err().to_string();
    assert!(error.starts_with("The model file") && error.contains("hero.fbx") && error.contains("261 characters"), "{error}");
    assert!(!work.join("hero.fbx").exists());
}

#[cfg(windows)]
#[test]
fn gltf_images_too_long_get_shorter_copies_and_buffers_stop_staging() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(folder.join(FBM)).unwrap();
    fs::write(folder.join(FBM).join(TEXTURE), b"color").unwrap();
    fs::write(folder.join("hero.bin"), b"data").unwrap();
    let gltf = folder.join("hero.gltf");
    let document = |buffer: &str| serde_json::json!({
        "asset": {"version": "2.0"},
        "buffers": [{"uri": buffer, "byteLength": 4}],
        // Written with "./" and another case, as some exporters do.
        "images": [{"uri": format!("./{FBM}/{}", TEXTURE.to_uppercase())}],
    });
    fs::write(&gltf, serde_json::to_vec(&document("hero.bin")).unwrap()).unwrap();
    let work = deep(temp.path(), 150);
    let local = source::stage_source(gltf.to_str().unwrap(), &work, None).unwrap();
    let entries = manifest(&work);
    let origin = format!("{FBM}/{TEXTURE}");
    assert_eq!(entries.iter().find(|e| e.relative == TEXTURE).unwrap().origin.as_deref(), Some(origin.as_str()));
    assert!(entries.iter().any(|e| e.relative == "hero.bin" && e.origin.is_none()));
    let proof = deadlock_preparation::provenance::source_proof(&gltf, &local).unwrap();
    deadlock_preparation::provenance::verify_files(gltf.to_str().unwrap(), gltf.to_str().unwrap(), &proof).unwrap();
    // Prepare again: the staged glTF still names the long path, and its copy
    // beside it stands for that image.
    let again = deep(&temp.path().join("x"), 150);
    let restaged = source::stage_source(local.to_str().unwrap(), &again, None).unwrap();
    let mut entries_again = manifest(&again);
    let entry = entries_again.iter_mut().find(|e| e.relative == TEXTURE).unwrap();
    assert_eq!(entry.found.take().as_deref(), Some(TEXTURE));
    assert_eq!(entries_again, entries);
    assert_eq!(fs::read(again.join(TEXTURE)).unwrap(), b"color");
    let proof = deadlock_preparation::provenance::source_proof(&local, &restaged).unwrap();
    deadlock_preparation::provenance::verify_files(local.to_str().unwrap(), local.to_str().unwrap(), &proof).unwrap();
    // A buffer Blender would read from a path too long.
    fs::rename(folder.join("hero.bin"), folder.join(FBM).join("hero.bin")).unwrap();
    fs::write(&gltf, serde_json::to_vec(&document(&format!("{FBM}/hero.bin"))).unwrap()).unwrap();
    let work = deep(&temp.path().join("y"), 200);
    let error = source::stage_source(gltf.to_str().unwrap(), &work, None).unwrap_err().to_string();
    assert!(error.contains("hero.bin") && error.contains("glTF data file"), "{error}");
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

// Images named under another extension (a model asking for `Body.tga` with
// only `Body.jpg` beside it) and materials with no image, matched by name.

fn leak(text: String) -> &'static [u8] {
    Box::leak(text.into_boxed_str()).as_bytes()
}
/// An FBX with `textures` (id, path or "" for none) and `materials` (id,
/// name), and texture-to-material connections `links` (texture id, material id).
fn textured(textures: &[(i64, &str)], materials: &[(i64, &str)], links: &[(i64, i64)]) -> Vec<u8> {
    let mut objects = vec![];
    for (id, name) in materials {
        objects.push(n("Material", vec![P::L(*id), P::S(leak(format!("{name}\x00\x01Material"))), P::S(b"")], vec![]));
    }
    for (id, path) in textures {
        let mut children = vec![n("Type", vec![P::S(b"TextureVideoClip")], vec![])];
        if !path.is_empty() {
            children.push(n("FileName", vec![P::S(leak(format!("F:/Artist/sourceimages/{path}")))], vec![]));
            children.push(n("RelativeFilename", vec![P::S(leak(format!("..\\Maya\\sourceimages\\{path}")))], vec![]));
        }
        objects.push(n("Texture", vec![P::L(*id), P::S(leak(format!("Map #{id}\x00\x01Texture"))), P::S(b"")], children));
    }
    let connections = links
        .iter()
        .map(|(texture, material)| n("C", vec![P::S(b"OP"), P::L(*texture), P::L(*material), P::S(b"DiffuseColor")], vec![]))
        .collect();
    document(7500, &[n("Objects", vec![], objects), n("Connections", vec![], connections)])
}
fn stage(fbx: &Path, work: &Path) -> std::path::PathBuf {
    source::stage_source(fbx.to_str().unwrap(), work, None).unwrap()
}
fn entry<'a>(entries: &'a [source::ManifestEntry], relative: &str) -> &'a source::ManifestEntry {
    entries.iter().find(|e| e.relative == relative).unwrap_or_else(|| panic!("{relative} in {entries:?}"))
}
fn verify(original: &Path, staged: &Path) -> deadlock_preparation::Result<()> {
    let proof = deadlock_preparation::provenance::source_proof(original, staged)?;
    let text = original.to_str().unwrap();
    deadlock_preparation::provenance::verify_files(text, text, &proof)
}

#[test]
fn the_reader_gives_the_paths_asked_for_and_which_materials_have_textures() {
    let bytes = textured(&[(10, "Body.tga"), (11, ""), (12, "Hair.psd")], &[(1, "MI_Body"), (2, "MI_Hair"), (3, "MI_Eye")], &[(10, 1), (11, 2), (12, 2)]);
    let summary = deadlock_preparation::fbx_images::read(Cursor::new(&bytes), bytes.len() as u64, &CancellationToken::new()).unwrap().unwrap();
    assert_eq!(summary.paths.into_iter().collect::<Vec<_>>(), [("body.tga".into(), "Body.tga".into()), ("hair.psd".into(), "Hair.psd".into())]);
    // The element names are searched for too, as before.
    assert!(summary.names.contains("map #11") && summary.names.contains("body.tga"));
    let materials: Vec<_> = summary.materials.iter().map(|m| (m.name.as_str(), m.textures.clone())).collect();
    assert_eq!(materials, [
        ("MI_Body", vec![Some("body.tga".to_owned())]),
        ("MI_Hair", vec![None, Some("hair.psd".to_owned())]),
        ("MI_Eye", vec![]),
    ]);
    assert!(!summary.materials[2].linked());
    // A record with neither properties nor children, as Blender's own FBX
    // exporter writes them, ends where its name ends; such files used to be
    // read as unknown, staging every image beside them.
    let empty = document(7400, &[n("Objects", vec![], vec![n("Pose", vec![], vec![]), n("Texture", vec![P::L(10), P::S(b"Map\x00\x01Texture"), P::S(b"")], vec![n("FileName", vec![P::S(b"Body.png")], vec![])])])]);
    assert_eq!(names(&empty), Some(set(&["body.png", "map"])));
}

#[cfg(windows)]
#[test]
fn an_image_saved_under_another_extension_stands_in_for_it() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(folder.join("tex")).unwrap();
    fs::write(folder.join("Body.jpg"), b"body jpeg").unwrap();
    fs::write(folder.join("tex/Face.png"), b"face png").unwrap();
    fs::write(folder.join("unrelated.png"), b"other").unwrap();
    let fbx = folder.join("hero.fbx");
    fs::write(&fbx, textured(&[(10, "Body.tga"), (11, "Face.psd")], &[(1, "MI_Body"), (2, "MI_Face")], &[(10, 1), (11, 2)])).unwrap();
    let work = temp.path().join("work");
    let local = stage(&fbx, &work);
    let entries = manifest(&work);
    assert_eq!(entries.len(), 3, "{entries:?}");
    // Beside the model under its own name, standing for the name the model asks for.
    let body = entry(&entries, "Body.jpg");
    assert_eq!((body.origin.as_deref(), body.found.as_deref()), (Some("Body.tga"), Some("Body.jpg")));
    let face = entry(&entries, "Face.png");
    assert_eq!((face.origin.as_deref(), face.found.as_deref()), (Some("tex/Face.psd"), Some("tex/Face.png")));
    assert!(source::stands_in(body) && source::stands_in(face) && body.materials.is_none());
    assert_eq!(fs::read(work.join("Face.png")).unwrap(), b"face png");
    assert!(!work.join("tex").exists());
    // The proof names the user's own files, and it verifies.
    verify(&fbx, &local).unwrap();
    fs::write(folder.join("tex/Face.png"), b"changed").unwrap();
    let error = verify(&fbx, &local).unwrap_err().to_string();
    assert!(error.contains("Changed or missing source input") && error.contains("Face.png"), "{error}");
    fs::write(folder.join("tex/Face.png"), b"face png").unwrap();
    // Prepare again imports the staged copy: the stand-ins stay stand-ins at
    // the same place (never JPEG bytes under the .tga name).
    let again = temp.path().join("again");
    let restaged = stage(&local, &again);
    let mut second = manifest(&again);
    assert!(!again.join("Body.tga").exists() && !again.join("tex").exists());
    assert_eq!(entry(&second, "Face.png").found.as_deref(), Some("Face.png"));
    verify(&local, &restaged).unwrap();
    for e in &mut second {
        if e.relative == "Face.png" {
            e.found = Some("tex/Face.png".into());
        }
    }
    let mut first = entries.clone();
    first.sort_by(|a, b| a.relative.cmp(&b.relative));
    second.sort_by(|a, b| a.relative.cmp(&b.relative));
    assert_eq!(second, first);
    // The manifest reads back to the same text.
    let text = fs::read_to_string(work.join("source-manifest.json")).unwrap();
    assert_eq!(serde_json::to_string_pretty(&entries).unwrap(), text.trim_end());
}

#[cfg(windows)]
#[test]
fn several_or_no_files_of_that_name_stand_in_for_nothing_on_their_own() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(folder.join("old")).unwrap();
    fs::write(folder.join("Body.jpg"), b"jpeg").unwrap();
    fs::write(folder.join("old/body.PNG"), b"png").unwrap();
    let fbx = folder.join("hero.fbx");
    fs::write(&fbx, textured(&[(10, "Body.tga"), (11, "Hair.tga")], &[(1, "MI_Body")], &[(10, 1)])).unwrap();
    let work = temp.path().join("work");
    let local = stage(&fbx, &work);
    let entries = manifest(&work);
    // Both are staged for the user to choose from, for the same name; the
    // Blender side uses neither by itself (staged_images.py `choices`).
    let origins: Vec<_> = entries.iter().filter(|e| source::stands_in(e)).map(|e| (e.relative.as_str(), e.origin.as_deref().unwrap())).collect();
    assert_eq!(origins, [("Body.jpg", "Body.tga"), ("body.PNG", "old/body.tga")]);
    verify(&fbx, &local).unwrap();
    // More than eight such files (here in nine folders): none is staged.
    for i in 0..8 {
        fs::create_dir_all(folder.join(format!("v{i}"))).unwrap();
        fs::write(folder.join(format!("v{i}/Body.png")), b"png").unwrap();
    }
    let work = temp.path().join("many");
    stage(&fbx, &work);
    assert!(manifest(&work).iter().all(|e| !source::stands_in(e)), "{:?}", manifest(&work));
    for i in 0..8 {
        fs::remove_dir_all(folder.join(format!("v{i}"))).unwrap();
    }
    // No file of the name at all: nothing but the model.
    fs::remove_file(folder.join("Body.jpg")).unwrap();
    fs::remove_file(folder.join("old/body.PNG")).unwrap();
    let work = temp.path().join("none");
    stage(&fbx, &work);
    assert_eq!(staged(&work), set(&["hero.fbx"]));
}

#[cfg(windows)]
#[test]
fn an_ordinary_textured_model_stages_exactly_as_before() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(&folder).unwrap();
    // Its image is found under the name it asks for; another image with the
    // material's name and one with a texture element's name lie beside it.
    fs::write(folder.join("Body_D.png"), b"body").unwrap();
    fs::write(folder.join("Body.png"), b"another body").unwrap();
    fs::write(folder.join("Body_D.jpg"), b"older body").unwrap();
    fs::write(folder.join("map #11.png"), b"element").unwrap();
    let fbx = folder.join("hero.fbx");
    fs::write(&fbx, textured(&[(10, "Body_D.png"), (11, "")], &[(1, "Body"), (2, "Skin")], &[(10, 1), (11, 2)])).unwrap();
    let work = temp.path().join("work");
    stage(&fbx, &work);
    let sha = |path: &Path| deadlock_preparation::digest(path).unwrap();
    let expected = format!(
        "[\n  {{\n    \"relative\": \"Body_D.png\",\n    \"sha256\": \"{}\"\n  }},\n  {{\n    \"relative\": \"hero.fbx\",\n    \"sha256\": \"{}\"\n  }}\n]",
        sha(&folder.join("Body_D.png")),
        sha(&fbx)
    );
    assert_eq!(fs::read_to_string(work.join("source-manifest.json")).unwrap().trim_end(), expected);
}

#[cfg(windows)]
#[test]
fn a_material_without_an_image_gets_the_one_named_like_it() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(folder.join("Programs")).unwrap();
    for name in ["T_Sample_Body.png", "T_Sample_Hair.png", "T_Sample_Hair_N.png", "T_Sample_Clothes.jpg", "T_Sample_Face.png"] {
        fs::write(folder.join(name), name.as_bytes()).unwrap();
    }
    fs::write(folder.join("Programs").join("wallpaper_Hair.png"), b"wallpaper").unwrap();
    // MI_Body has its image, MI_Hair and MI_Clothes none, MI_Face one that is missing.
    let fbx = folder.join("Sample.fbx");
    fs::write(&fbx, textured(&[(10, "T_Sample_Body.png"), (11, "Face_old.psd")], &[(1, "MI_Body"), (2, "MI_Hair"), (3, "MI_Clothes"), (4, "MI_Face")], &[(10, 1), (11, 4)])).unwrap();
    let work = temp.path().join("work");
    let local = stage(&fbx, &work);
    let entries = manifest(&work);
    let annotated: Vec<_> = entries.iter().map(|e| (e.relative.as_str(), e.materials.clone().unwrap_or_default())).collect();
    let none = Vec::<String>::new;
    assert_eq!(annotated, [
        ("Sample.fbx", none()),
        ("T_Sample_Body.png", none()),
        ("T_Sample_Clothes.jpg", vec!["MI_Clothes".to_owned()]),
        ("T_Sample_Face.png", vec!["MI_Face".to_owned()]),
        ("T_Sample_Hair.png", vec!["MI_Hair".to_owned()]),
    ]);
    // Staged where they are, proved there.
    assert!(entries.iter().all(|e| e.origin.is_none() && e.found.is_none()));
    verify(&fbx, &local).unwrap();
    // Prepare again finds them beside the staged copy, with the same notes.
    let again = temp.path().join("again");
    let restaged = stage(&local, &again);
    assert_eq!(manifest(&again), entries);
    verify(&local, &restaged).unwrap();
    // The note reads back.
    let raw: Vec<serde_json::Value> = serde_json::from_slice(&fs::read(work.join("source-manifest.json")).unwrap()).unwrap();
    assert_eq!(raw[4]["materials"], serde_json::json!(["MI_Hair"]));
}

#[cfg(windows)]
#[test]
fn gltf_images_under_another_extension_and_materials_by_name() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("model");
    fs::create_dir_all(folder.join("tex")).unwrap();
    fs::write(folder.join("tex/body.jpg"), b"body jpeg").unwrap();
    fs::write(folder.join("Hair_BaseColor.png"), b"hair").unwrap();
    fs::write(folder.join("hero.bin"), b"data").unwrap();
    let gltf = folder.join("hero.gltf");
    let document = |uri: &str| serde_json::json!({
        "asset": {"version": "2.0"},
        "buffers": [{"uri": "hero.bin", "byteLength": 4}],
        "images": [{"uri": uri}],
        "textures": [{"source": 0}],
        "materials": [
            {"name": "Body", "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}},
            {"name": "Hair", "pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1]}},
        ],
    });
    fs::write(&gltf, serde_json::to_vec(&document("./tex/body.png")).unwrap()).unwrap();
    let work = temp.path().join("work");
    let local = stage(&gltf, &work);
    let entries = manifest(&work);
    let body = entry(&entries, "body.jpg");
    assert_eq!((body.origin.as_deref(), body.found.as_deref()), (Some("tex/body.png"), Some("tex/body.jpg")));
    assert_eq!(entry(&entries, "Hair_BaseColor.png").materials, Some(vec!["Hair".to_owned()]));
    verify(&gltf, &local).unwrap();
    // Prepare again keeps both.
    let again = temp.path().join("again");
    let restaged = stage(&local, &again);
    let second = manifest(&again);
    assert_eq!(entry(&second, "body.jpg").origin.as_deref(), Some("tex/body.png"));
    assert_eq!(entry(&second, "Hair_BaseColor.png").materials, Some(vec!["Hair".to_owned()]));
    verify(&local, &restaged).unwrap();
    // Several files of that name: staging stops and names them.
    fs::write(folder.join("tex/body.tga"), b"body tga").unwrap();
    let error = source::stage_source(gltf.to_str().unwrap(), &temp.path().join("several"), None).unwrap_err().to_string();
    assert!(error.contains("./tex/body.png") && error.contains("body.jpg, body.tga"), "{error}");
    // None: the message it always gave.
    fs::write(&gltf, serde_json::to_vec(&document("tex/face.png")).unwrap()).unwrap();
    let error = source::stage_source(gltf.to_str().unwrap(), &temp.path().join("none"), None).unwrap_err().to_string();
    assert_eq!(error, "Missing or escaping glTF dependency: tex/face.png");
}
