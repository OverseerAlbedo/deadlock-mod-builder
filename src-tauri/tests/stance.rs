//! The hero's holding pose in the gun fit preview, read from real game files
//! (run with --ignored; needs DMB_PAK and DMB_HAZE_BASE, the decompiled
//! sources' `base` folder of Haze).
#[test]
#[ignore = "needs the Deadlock game archive and Haze's decompiled sources"]
fn haze_holds_both_guns_in_front_of_her_in_the_stance() {
    let (Ok(pak), Ok(base)) = (std::env::var("DMB_PAK"), std::env::var("DMB_HAZE_BASE")) else { return };
    let base = std::path::PathBuf::from(base);
    let raw = deadlock_vpk::Vpk::open(&pak).unwrap().read("models/heroes_staging/haze/haze.vmdl_c").unwrap();
    let data = deadlock_resource::container::read_blocks(&raw, "DATA").unwrap();
    let skeleton = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
    let (target, _) = deadlock_skeleton::transforms::prepare_target(&skeleton).unwrap();
    let text = std::fs::read_to_string(base.join("models/heroes_staging/haze/haze.vmdl")).unwrap();
    let reference = deadlock_preparation::weapon::reference_from_sources(&base, &text, &target, false, &deadlock_core::CancellationToken::new()).unwrap();
    let stance = reference.stance.as_ref().expect("Haze has a holding pose");
    println!("clip {}", stance.clip);
    let bounds = |positions: &[f32]| {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in positions.chunks_exact(3) {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        (lo, hi)
    };
    println!("rest body {:?}", bounds(&reference.body_shape.positions));
    println!("held body {:?}", bounds(&stance.body_shape.positions));
    for side in &reference.sides {
        let m = stance.hands.get(&side.side).expect("each hand moves its gun");
        let n = side.points.len() as f64;
        let c = [0, 1, 2].map(|i| side.points.iter().map(|p| p[i]).sum::<f64>() / n);
        let held = [0, 1, 2].map(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2] + m[i][3]);
        println!("{} gun centre rest {c:?} held {held:?}", side.side);
        // The held body is not the rest body, and the guns stay near the body.
        let (lo, hi) = bounds(&stance.body_shape.positions);
        assert!((0..3).all(|i| f64::from(lo[i]) - 30. < held[i] && held[i] < f64::from(hi[i]) + 30.), "{held:?} {lo:?} {hi:?}");
    }
    assert_ne!(reference.body_shape.positions, stance.body_shape.positions);
}

/// Other heroes stand upright with their gun(s) in the holding pose. DMB_STANCE_CASES
/// lists `<sources base>*<model path without extension>` separated by `;`.
#[test]
#[ignore = "needs the Deadlock game archive and decompiled sources"]
fn other_heroes_stand_upright_with_their_guns() {
    let (Ok(pak), Ok(cases)) = (std::env::var("DMB_PAK"), std::env::var("DMB_STANCE_CASES")) else { return };
    let vpk = deadlock_vpk::Vpk::open(&pak).unwrap();
    for case in cases.split(';').filter(|c| !c.is_empty()) {
        let (base, model) = case.split_once('*').unwrap();
        let base = std::path::PathBuf::from(base);
        let raw = vpk.read(&format!("{model}.vmdl_c")).unwrap();
        let data = deadlock_resource::container::read_blocks(&raw, "DATA").unwrap();
        let skeleton = deadlock_skeleton::Skeleton::from_value(data[0].root.get("m_modelSkeleton").unwrap()).unwrap();
        let (target, _) = deadlock_skeleton::transforms::prepare_target(&skeleton).unwrap();
        let text = std::fs::read_to_string(base.join(format!("{model}.vmdl"))).unwrap();
        let reference = deadlock_preparation::weapon::reference_from_sources(&base, &text, &target, false, &deadlock_core::CancellationToken::new()).unwrap();
        let stance = reference.stance.as_ref().unwrap_or_else(|| panic!("{model} has a holding pose"));
        let height = |positions: &[f32]| {
            let z = positions.chunks_exact(3).map(|p| p[2]);
            z.clone().fold(f32::MIN, f32::max) - z.fold(f32::MAX, f32::min)
        };
        let (rest, held) = (height(&reference.body_shape.positions), height(&stance.body_shape.positions));
        println!("{model}: clip {} rest height {rest} held height {held} hands {:?}", stance.clip, stance.hands.keys().collect::<Vec<_>>());
        assert!(held > rest * 0.7 && held < rest * 1.15, "{model} is not upright: {held} vs {rest}");
        assert!(!stance.hands.is_empty() || reference.points.is_empty(), "{model} gun has no hand");
    }
}
