//! MMD models (.pmx, .pmd) are staged as the packed Blender file MMD Tools
//! makes of them (`BlendStager::stage_mmd`, run by Blender in the app).
use deadlock_preparation::{
    provenance,
    source::{self, BlendStager, ManifestEntry, PackedImage},
};
use std::{cell::RefCell, collections::BTreeMap, fs, path::Path, path::PathBuf};

/// Stands in for Blender: records the call and writes a staged file.
#[derive(Default)]
struct FakeBlender {
    calls: RefCell<Vec<(&'static str, PathBuf, PathBuf)>>,
    images: Vec<PackedImage>,
}
impl BlendStager for FakeBlender {
    fn stage_blend(&self, source: &Path, destination: &Path) -> deadlock_preparation::Result<Vec<PackedImage>> {
        self.calls.borrow_mut().push(("blend", source.into(), destination.into()));
        fs::write(destination, b"BLENDER-v").unwrap();
        Ok(vec![])
    }
    fn stage_mmd(&self, source: &Path, destination: &Path) -> deadlock_preparation::Result<Vec<PackedImage>> {
        self.calls.borrow_mut().push(("mmd", source.into(), destination.into()));
        fs::write(destination, b"BLENDER-v packed").unwrap();
        Ok(self.images.clone())
    }
}
fn image(origin: &Path, sha256: &str) -> PackedImage {
    PackedImage { origin: origin.to_str().unwrap().into(), sha256: sha256.into(), details: BTreeMap::new() }
}
fn manifest(work: &Path) -> Vec<ManifestEntry> {
    serde_json::from_slice(&fs::read(work.join("source-manifest.json")).unwrap()).unwrap()
}

#[test]
fn an_mmd_model_is_staged_as_its_packed_blender_file() {
    let temp = tempfile::tempdir().unwrap();
    // Non-ASCII names are the normal case for MMD models.
    let folder = temp.path().join("模型").join("キャラ");
    fs::create_dir_all(folder.join("tex")).unwrap();
    let pmx = folder.join("キャラ.pmx");
    fs::write(&pmx, b"PMX \x00\x00\x00\x40").unwrap();
    let texture = folder.join("tex").join("肌.png");
    fs::write(&texture, b"skin").unwrap();
    let toon = temp.path().join("addon").join("toon01.bmp");
    let blender = FakeBlender { images: vec![image(&texture, "aa"), image(&toon, "bb")], ..Default::default() };
    let work = temp.path().join("work");
    let local = source::stage_source(pmx.to_str().unwrap(), &work, Some(&blender)).unwrap();
    assert_eq!(local, work.join("キャラ.blend"));
    assert_eq!(blender.calls.borrow().as_slice(), [("mmd", pmx.clone(), local.clone())]);
    // One entry, named like the model, bound to the model's own digest.
    let entries = manifest(&work);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].relative, "キャラ.pmx");
    assert_eq!(entries[0].transform.as_deref(), Some(source::MMD_TRANSFORM));
    assert_eq!(entries[0].original_sha256.as_deref(), Some(deadlock_preparation::digest(&pmx).unwrap().as_str()));
    assert_eq!(entries[0].sha256, deadlock_preparation::digest(&local).unwrap());
    // The proof names the model itself and every image Blender packed,
    // relative to the model's folder (also the add-on's shared toons outside it).
    let proof = provenance::source_proof(&pmx, &local).unwrap();
    let relative: Vec<_> = proof.iter().map(|p| p.relative.replace('\\', "/")).collect();
    assert_eq!(relative[0], "");
    assert_eq!(relative[1], "tex/肌.png");
    assert!(relative[2].ends_with("addon/toon01.bmp") && relative[2].starts_with("../"), "{relative:?}");
    // Nothing was copied besides the Blender file and the manifest.
    let mut names: Vec<_> = fs::read_dir(&work).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["source-manifest.json", "キャラ.blend"]);
}

#[test]
fn pmd_models_and_upper_case_suffixes_go_the_same_way() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["old.pmd", "MODEL.PMX"] {
        let file = temp.path().join(name);
        fs::write(&file, b"Pmd").unwrap();
        let blender = FakeBlender::default();
        let work = temp.path().join(format!("work-{name}"));
        let local = source::stage_source(file.to_str().unwrap(), &work, Some(&blender)).unwrap();
        assert_eq!(local.extension().unwrap(), "blend", "{name}");
        assert_eq!(blender.calls.borrow()[0].0, "mmd");
    }
}

#[test]
fn an_mmd_model_needs_blender() {
    let temp = tempfile::tempdir().unwrap();
    let pmx = temp.path().join("a.pmx");
    fs::write(&pmx, b"PMX ").unwrap();
    let error = source::stage_source(pmx.to_str().unwrap(), &temp.path().join("w"), None).unwrap_err().to_string();
    assert!(error.contains("MMD Tools"), "{error}");
}

#[test]
fn mmd_files_are_accepted_sources() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["a.pmx", "b.pmd", "c.PMX"] {
        let file = temp.path().join(name);
        fs::write(&file, b"x").unwrap();
        assert!(source::resolve_source(file.to_str().unwrap()).is_ok(), "{name}");
    }
    let other = temp.path().join("d.vmd");
    fs::write(&other, b"x").unwrap();
    let error = source::resolve_source(other.to_str().unwrap()).unwrap_err().to_string();
    assert!(error.contains("MMD (.pmx, .pmd)"), "{error}");
}

#[cfg(windows)]
#[test]
fn a_model_or_copy_path_too_long_for_blender_stops_before_blender_runs() {
    let temp = tempfile::tempdir().unwrap();
    let pmx = temp.path().join("m.pmx");
    fs::write(&pmx, b"PMX ").unwrap();
    // A workspace deep enough for the copy to pass 260 characters on any PC.
    let work = temp.path().join("w".repeat(120)).join("v".repeat(120));
    let blender = FakeBlender::default();
    let error = source::stage_source(pmx.to_str().unwrap(), &work, Some(&blender)).unwrap_err().to_string();
    assert!(error.starts_with("Its Blender copy") && error.contains("too long for Blender: its path is"), "{error}");
    assert!(error.contains("Workspace folder in Settings"), "{error}");
    assert!(blender.calls.borrow().is_empty());
    // The model's own path.
    let deep = temp.path().join("m".repeat(120)).join("n".repeat(120));
    fs::create_dir_all(&deep).unwrap();
    let long = deep.join("model.pmx");
    fs::write(&long, b"PMX ").unwrap();
    let error = source::stage_source(long.to_str().unwrap(), &temp.path().join("w2"), Some(&blender)).unwrap_err().to_string();
    assert!(error.starts_with("The model file") && error.contains("too long for Blender"), "{error}");
    assert!(blender.calls.borrow().is_empty());
}
