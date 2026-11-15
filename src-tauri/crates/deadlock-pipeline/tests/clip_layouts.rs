//! Source 2 Viewer 21.1 writes a hero's clip takes where each `.vnmclip`
//! names them instead of beside the clip. The gun stance and the movement
//! preview must pick the same clips from either layout and play the same
//! frames. Run with --ignored; DMB_CLIPS_OLD and DMB_CLIPS_NEW are folders with
//! one decompile per hero (`<hero>/models/...`), the first made by an older
//! Source 2 Viewer, the second by 21.1 (same command, same game files).
use deadlock_preparation::stance;
use std::path::{Path, PathBuf};

fn vmdl(hero: &Path) -> Option<PathBuf> {
    fn walk(folder: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|x| x == "vmdl") {
                out.push(path);
            }
        }
    }
    let mut found = vec![];
    walk(&hero.join("models"), &mut found);
    found.into_iter().max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
}

fn stem(path: Option<&PathBuf>) -> String {
    path.and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "(none)".into())
}

/// The clip names the gun stance and each preview part would play.
fn picks(base: &Path) -> Option<Vec<(String, String)>> {
    let model = vmdl(base)?;
    let doc = deadlock_resource::text::Document::parse(std::fs::read_to_string(&model).ok()?).ok()?;
    let mesh = doc.objects(Some("RenderMeshFile")).iter().find_map(|n| n.get("filename").and_then(|f| f.scalar()).map(str::to_string))?;
    let mesh_folder = base.join(mesh).parent()?.to_path_buf();
    let mut out = vec![("stance".to_string(), stem(stance::find_clip(base, &mesh_folder).as_ref()))];
    for (name, parts) in deadlock_pipeline::motion_preview::clips_for(base, model.parent()?) {
        for (i, part) in parts.iter().enumerate() {
            let played = part.candidates.iter().find(|file| stance::frames(file, 30., 60).is_some_and(|(_, poses)| !poses.is_empty()));
            out.push((format!("{name}[{i}]"), stem(played)));
        }
    }
    Some(out)
}

/// Largest joint-position difference between two poses (joints in both).
fn pose_gap(a: &std::collections::BTreeMap<String, [[f64; 4]; 4]>, b: &std::collections::BTreeMap<String, [[f64; 4]; 4]>) -> f64 {
    a.iter().filter_map(|(name, m)| Some((m, b.get(name)?))).map(|(m, n)| (0..3).map(|i| (m[i][3] - n[i][3]).abs()).fold(0., f64::max)).fold(0., f64::max)
}

#[test]
#[ignore = "needs two decompiles of the same heroes, by an older Source 2 Viewer and by 21.1"]
fn both_decompile_layouts_give_the_same_clips() {
    let (Some(old), Some(new)) = (std::env::var_os("DMB_CLIPS_OLD"), std::env::var_os("DMB_CLIPS_NEW")) else { return };
    let (old, new) = (PathBuf::from(old), PathBuf::from(new));
    let mut heroes = 0;
    let mut different = vec![];
    for entry in std::fs::read_dir(&new).unwrap().flatten() {
        let hero = entry.file_name();
        let (Some(a), Some(b)) = (picks(&old.join(&hero)), picks(&entry.path())) else { continue };
        heroes += 1;
        for ((part, x), (_, y)) in a.iter().zip(&b) {
            println!("{:<10} {part:<10} older {x:<32} 21.1 {y}", hero.to_string_lossy());
            if x != y {
                different.push(format!("{} {part}: {x} vs {y}", hero.to_string_lossy()));
            }
        }
        assert_eq!(a.len(), b.len(), "{}: preview parts differ", hero.to_string_lossy());
        // A clip that plays a frame range of a shared take (21.1) shows the
        // frames the older layout wrote as the clip's own DMX (within 0.05 in;
        // Harrow's reload_quick_healing_needles_idle differs by 0.014 in).
        let clips = entry.path().join("models");
        let mut ranged = vec![];
        fn walk(folder: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(folder).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "vnmclip") && std::fs::read_to_string(&p).is_ok_and(|t| t.contains("m_nStartFrame")) {
                    out.push(p);
                }
            }
        }
        walk(&clips, &mut ranged);
        for clip in ranged {
            let relative = clip.strip_prefix(entry.path()).unwrap();
            let own = old.join(&hero).join(relative).with_extension("dmx");
            let (Some((_, before)), Some((_, after))) = (stance::frames(&own, 30., 1000), stance::frames(&clip, 30., 1000)) else { continue };
            let gap = [0, before.len().min(after.len()) - 1].iter().map(|&f| pose_gap(&before[f], &after[f])).fold(0., f64::max);
            println!("{:<10} range {:<40} frames {} vs {}, first/last pose gap {gap:.5}", hero.to_string_lossy(), relative.display(), before.len(), after.len());
            assert!(before.len().abs_diff(after.len()) <= 1 && gap < 0.05, "{}: {} vs {} frames, gap {gap}", relative.display(), before.len(), after.len());
        }
    }
    assert!(heroes > 0, "no hero decompiled in both folders");
    assert!(different.is_empty(), "{}", different.join("\n"));
}
