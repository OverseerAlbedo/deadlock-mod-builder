use deadlock_core::{safety, CancellationToken};
use deadlock_materials::*;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn setup(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let content = root.join("content");
    fs::create_dir(&content).unwrap();
    fs::write(
        content.join("a.vmat"),
        "Layer0 { shader pbr.vfx TextureColor1 \"a.png\" }",
    )
    .unwrap();
    fs::write(content.join("a.png"), "texture v1").unwrap();
    let compiler = root.join("bin/resourcecompiler.exe");
    fs::create_dir_all(compiler.parent().unwrap()).unwrap();
    fs::write(&compiler, "compiler fixture not executable").unwrap();
    let csdk = root.join("csdk");
    fs::create_dir_all(csdk.join("game/core")).unwrap();
    fs::write(csdk.join("game/core/shaders_vpk"), "shaders").unwrap();
    (content, compiler, csdk)
}
#[test]
fn material_cache_invalidates_texture_compiler_and_shader_inputs() {
    let temp = tempfile::tempdir().unwrap();
    let (content, compiler, csdk) = setup(temp.path());
    let cancel = CancellationToken::new();
    let first = plan_cache("a.vmat", &content, &compiler, &csdk, &cancel).unwrap();
    assert!(first.cacheable);
    fs::write(content.join("a.png"), "texture v2").unwrap();
    let second = plan_cache("a.vmat", &content, &compiler, &csdk, &cancel).unwrap();
    assert_ne!(first.key, second.key);
    fs::write(&compiler, "compiler v2").unwrap();
    let third = plan_cache("a.vmat", &content, &compiler, &csdk, &cancel).unwrap();
    assert_ne!(second.key, third.key);
    fs::write(csdk.join("game/core/shaders_vpk"), "new shader bytes").unwrap();
    let fourth = plan_cache("a.vmat", &content, &compiler, &csdk, &cancel).unwrap();
    assert_ne!(third.key, fourth.key);
    fs::create_dir_all(csdk.join("game/citadel")).unwrap();
    fs::write(csdk.join("game/citadel/gameinfo.gi"), "GameInfo v1").unwrap();
    let explicit = plan_cache("a.vmat", &content, &compiler, &csdk, &cancel).unwrap();
    assert_ne!(fourth.key, explicit.key);
    fs::write(csdk.join("game/citadel/gameinfo.gi"), "GameInfo v2").unwrap();
    let changed = plan_cache("a.vmat", &content, &compiler, &csdk, &cancel).unwrap();
    assert_ne!(explicit.key, changed.key);
}
#[test]
fn nested_source_programs_and_missing_textures_disable_cache() {
    let temp = tempfile::tempdir().unwrap();
    let (content, compiler, csdk) = setup(temp.path());
    for reference in ["nested.vmat", "texture.vtex", "missing.png"] {
        fs::write(
            content.join("a.vmat"),
            format!("Layer0 {{ TextureColor1 \"{reference}\" }}"),
        )
        .unwrap();
        assert!(
            !plan_cache(
                "a.vmat",
                &content,
                &compiler,
                &csdk,
                &CancellationToken::new()
            )
            .unwrap()
            .cacheable
        );
    }
}
fn compiled(root: &Path) -> PathBuf {
    let game = root.join("game");
    fs::create_dir_all(game.join("materials")).unwrap();
    fs::copy(
        fixtures().join("compiled-material.bin"),
        game.join("a.vmat_c"),
    )
    .unwrap();
    fs::copy(
        fixtures().join("compiled-texture.bin"),
        game.join("materials/shared.vtex_c"),
    )
    .unwrap();
    game
}
#[test]
fn cache_preserves_unchanged_shared_dependency_and_verified_restore() {
    let temp = tempfile::tempdir().unwrap();
    let game = compiled(temp.path());
    let cancel = CancellationToken::new();
    let before = compiled_inventory(&game, &cancel).unwrap();
    let cache = temp.path().join("cache/key");
    let stored = store_compiled(&cache, "a.vmat", &game, &before, &cancel).unwrap();
    assert!(stored.cacheable);
    assert_eq!(stored.files.len(), 2);
    assert!(stored.files.contains_key("materials/shared.vtex_c"));
    let restored = temp.path().join("restored");
    assert!(restore_cached(&cache, "a.vmat", &restored, &cancel).unwrap());
    for (rel, sha) in stored.files {
        assert_eq!(safety::digest(&restored.join(rel)).unwrap(), sha);
    }
}
#[test]
fn corrupt_cache_misses_without_touching_existing_generated_output() {
    let temp = tempfile::tempdir().unwrap();
    let game = compiled(temp.path());
    let cancel = CancellationToken::new();
    let cache = temp.path().join("cache/key");
    store_compiled(&cache, "a.vmat", &game, &Hashes::new(), &cancel).unwrap();
    fs::write(cache.join("files/materials/shared.vtex_c"), "corrupt").unwrap();
    fs::write(game.join("a.vmat_c"), "preserve prior output").unwrap();
    assert!(!restore_cached(&cache, "a.vmat", &game, &cancel).unwrap());
    assert_eq!(
        fs::read(game.join("a.vmat_c")).unwrap(),
        b"preserve prior output"
    );
}
#[test]
fn missing_compiled_graph_dependency_disables_cache_and_no_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let game = compiled(temp.path());
    fs::remove_file(game.join("materials/shared.vtex_c")).unwrap();
    let cache = temp.path().join("cache/key");
    let receipt = store_compiled(
        &cache,
        "a.vmat",
        &game,
        &Hashes::new(),
        &CancellationToken::new(),
    )
    .unwrap();
    assert!(!receipt.cacheable);
    assert_eq!(receipt.missing_dependencies, ["materials/shared.vtex_c"]);
    assert!(!cache.exists());
}
#[test]
fn altered_manifest_omitting_dependency_cannot_be_a_cache_hit() {
    let temp = tempfile::tempdir().unwrap();
    let game = compiled(temp.path());
    let cancel = CancellationToken::new();
    let cache = temp.path().join("cache/key");
    let receipt = store_compiled(&cache, "a.vmat", &game, &Hashes::new(), &cancel).unwrap();
    let mut hashes = receipt.files;
    hashes.remove("materials/shared.vtex_c");
    fs::write(
        cache.join("manifest.json"),
        serde_json::to_vec(&hashes).unwrap(),
    )
    .unwrap();
    assert!(!restore_cached(&cache, "a.vmat", &game, &cancel).unwrap());
}
#[test]
fn invalid_cache_paths_and_cancel_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let game = compiled(temp.path());
    let cache = temp.path().join("cache");
    fs::create_dir(&cache).unwrap();
    fs::write(
        cache.join("manifest.json"),
        r#"{"a.vmat_c":"x","../escape_c":"y"}"#,
    )
    .unwrap();
    let result = restore_cached(&cache, "a.vmat", &game, &CancellationToken::new());
    assert!(result.is_err());
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(store_compiled(&cache, "a.vmat", &game, &Hashes::new(), &cancel).is_err());
}
