use crate::native::*;
use deadlock_resource::{binary, container, Kind, Value};
use deadlock_skeleton::Skeleton;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MODEL: &str = "models/heroes_staging/haze/haze.vmdl_c";
pub const SECONDARY: &str = "models/dmb_tests/streamer_playback_r1/left_streamer.vnmskel";
pub const ANCHOR: &str = "spine_3";
pub fn names() -> Vec<String> {
    (1..=7)
        .map(|j| format!("dmb_secondary_spring_base_L_a_{j:02}_jnt"))
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub schema: u32,
    pub fps: u32,
    pub duration_seconds: f64,
    pub bone_names: Vec<String>,
    /// Blender-evaluated local pose deltas, xyzw; endpoints form a closed loop.
    pub rotations: Vec<Vec<[f64; 4]>>,
}
impl Motion {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema == 1
                && self.fps == 30
                && self.duration_seconds == 4.0
                && self.bone_names == names(),
            "Wrong authored animation template",
        )?;
        require(
            self.rotations.len() == 121,
            "Expected 121 Blender-evaluated frames",
        )?;
        let mut changing = false;
        for frame in &self.rotations {
            require(frame.len() == 7, "Animation needs seven streamer joints")?;
            for q in frame {
                require(
                    q.iter().all(|x| x.is_finite())
                        && (q.iter().map(|x| x * x).sum::<f64>() - 1.).abs() < 1e-5,
                    "Invalid authored rotation",
                )?;
                if q[..3].iter().any(|x| x.abs() > 0.05) {
                    changing = true;
                }
            }
        }
        require(changing, "Animation has no visible motion")?;
        for j in 0..7 {
            require(
                qdistance(self.rotations[0][j], self.rotations[120][j]) < 1e-5,
                "Animation loop is discontinuous",
            )?;
        }
        Ok(())
    }
    pub fn sample(&self, t: f64, j: usize) -> [f64; 4] {
        let p = t.clamp(0., 1.) * 120.;
        let lo = (p.floor() as usize).min(120);
        let hi = (lo + 1).min(120);
        let a = self.rotations[lo][j];
        let mut b = self.rotations[hi][j];
        if a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>() < 0. {
            b = b.map(|x| -x);
        }
        normalize(std::array::from_fn(|i| {
            a[i] + (b[i] - a[i]) * (p - lo as f64)
        }))
    }
}
pub fn normalize(q: [f64; 4]) -> [f64; 4] {
    let n = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    q.map(|x| x / n)
}
pub fn multiply(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let [x, y, z, w] = a;
    let [bx, by, bz, bw] = b;
    normalize([
        w * bx + x * bw + y * bz - z * by,
        w * by - x * bz + y * bw + z * bx,
        w * bz + x * by - y * bx + z * bw,
        w * bw - x * bx - y * by - z * bz,
    ])
}
pub(crate) fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let [x, y, z, w] = q;
    let [a, b, c] = v;
    [
        (1. - 2. * (y * y + z * z)) * a + 2. * (x * y - z * w) * b + 2. * (x * z + y * w) * c,
        2. * (x * y + z * w) * a + (1. - 2. * (x * x + z * z)) * b + 2. * (y * z - x * w) * c,
        2. * (x * z - y * w) * a + 2. * (y * z + x * w) * b + (1. - 2. * (x * x + y * y)) * c,
    ]
}
pub fn qdistance(a: [f64; 4], b: [f64; 4]) -> f64 {
    let d = a
        .iter()
        .zip(b)
        .map(|(a, b)| a * b)
        .sum::<f64>()
        .abs()
        .min(1.);
    2. * d.acos()
}
pub fn pack(q: [f64; 4]) -> [u16; 3] {
    let mut q = normalize(q);
    let largest = (0..4)
        .max_by(|&a, &b| q[a].abs().total_cmp(&q[b].abs()))
        .unwrap();
    if q[largest] < 0. {
        q = q.map(|x| -x);
    }
    let mut out = [0u16; 3];
    let mut at = 0;
    for (j, x) in q.iter().enumerate() {
        if j != largest {
            out[at] = ((x + 1. / 2f64.sqrt()) / (2f64.sqrt()) * 32767.)
                .round()
                .clamp(0., 32767.) as u16;
            at += 1;
        }
    }
    out[0] |= ((largest as u16 >> 1) & 1) << 15;
    out[1] |= (largest as u16 & 1) << 15;
    out
}
pub fn unpack(p: [u16; 3]) -> [f64; 4] {
    let largest = ((p[0] >> 14) & 2 | p[1] >> 15) as usize;
    let c = p.map(|x| (x & 32767) as f64 * 2f64.sqrt() / 32767. - 1. / 2f64.sqrt());
    let omitted = (1. - c.iter().map(|x| x * x).sum::<f64>()).max(0.).sqrt();
    let mut at = 0;
    std::array::from_fn(|j| {
        if j == largest {
            omitted
        } else {
            let x = c[at];
            at += 1;
            x
        }
    })
}
pub fn selected(skel: &Skeleton) -> Result<Vec<usize>> {
    let indices = names()
        .iter()
        .map(|n| {
            skel.names
                .iter()
                .position(|s| s == n)
                .ok_or_else(|| format!("TangTang test bone missing: {n}").into())
        })
        .collect::<Result<Vec<_>>>()?;
    for (j, &index) in indices.iter().enumerate() {
        let parent = skel.parents[index];
        require(parent >= 0, "Streamer has no anchor")?;
        let expected = if j == 0 {
            ANCHOR
        } else {
            &skel.names[indices[j - 1]]
        };
        require(
            skel.names[parent as usize] == expected,
            "Streamer hierarchy differs from test template",
        )?;
        require(
            (skel.scales[index] - 1.).abs() < 1e-5,
            "Streamer local scale needs separate review",
        )?;
    }
    Ok(indices)
}

/// Legacy R1/R2 output-calculation removal. This alone did NOT disconnect the
/// name/hash binding and failed in the user's game. Kept for regression history.
pub fn prepare_model(raw: &[u8]) -> Result<(Vec<u8>, Skeleton, serde_json::Value)> {
    prepare_model_binding(raw, true)
}
/// Direct-primary revision: the mesh keeps its single existing animation
/// skeleton reference. R1's separate prop resource is not introduced.
pub fn prepare_primary_model(raw: &[u8]) -> Result<(Vec<u8>, Skeleton, serde_json::Value)> {
    let (model, skeleton, mut receipt) = prepare_model_binding(raw, false)?;
    let (model, binding) = crate::disconnect::unbind(&model, &skeleton)?;
    receipt["control_binding"] = binding;
    receipt["output_sha256"] = serde_json::json!(hash(&model));
    Ok((model, skeleton, receipt))
}
fn prepare_model_binding(
    raw: &[u8],
    secondary: bool,
) -> Result<(Vec<u8>, Skeleton, serde_json::Value)> {
    let before = data(raw, "DATA")?;
    let mut changed = before.clone();
    let skel = Skeleton::from_value(field(&before.root, "m_modelSkeleton")?)?;
    let indices = selected(&skel)?;
    let mut targets = BTreeSet::new();
    for &j in &indices {
        targets.insert(skel.names[j].clone());
        targets.insert(format!("{}___", skel.names[j]));
    }
    require(
        targets.iter().all(|n| skel.names.contains(n)),
        "The known streamer helper chain is incomplete",
    )?;
    let phys_before = data(raw, "PHYS")?;
    let mut phys = phys_before.clone();
    let fe = field(&phys_before.root, "m_pFeModel")?;
    for k in [
        "m_SimdNodeBases",
        "m_FitMatrices",
        "m_FitWeights",
        "m_JiggleBones",
        "m_CtrlSoftOffsets",
        "m_CtrlOsOffsets",
        "m_FollowNodes",
        "m_BoneMergeLinks",
        "m_DynKinLinks",
    ] {
        require(
            arr(fe, k)?.is_empty(),
            &format!("Unreviewed cloth output path: {k}"),
        )?;
    }
    require(
        field(fe, "m_nNodeBaseJiggleboneDependsCount")?.integer()? == 0,
        "Jiggle-dependent node bases require a different test",
    )?;
    let controls = arr(fe, "m_CtrlName")?;
    let target_controls: BTreeSet<_> = controls
        .iter()
        .enumerate()
        .filter_map(|(j, n)| n.string().ok().filter(|n| targets.contains(*n)).map(|_| j))
        .collect();
    require(
        target_controls.len() == 14,
        "Expected 14 named streamer physics controls",
    )?;
    let fe_changed = phys.root.get_mut("m_pFeModel").ok_or("No FE model")?;
    let mut removed = BTreeMap::new();
    for (key, index_key) in [("m_NodeBases", "nNode"), ("m_ReverseOffsets", "nBoneCtrl")] {
        let rows = arr(fe, key)?;
        let mut kept = vec![];
        let mut seen = BTreeSet::new();
        for row in rows {
            let j = usize::try_from(field(&row, index_key)?.integer()?)?;
            if target_controls.contains(&j) {
                require(seen.insert(j), "Duplicate streamer physics output")?;
            } else {
                kept.push(row);
            }
        }
        require(seen == target_controls, "Missing streamer physics outputs")?;
        removed.insert(key, seen.len());
        set(fe_changed, key, list(kept))?;
    }
    unchanged_except(fe, fe_changed, &["m_NodeBases", "m_ReverseOffsets"])?;
    unchanged_except(&phys_before.root, &phys.root, &["m_pFeModel"])?;
    let mut flags = arr(field(&before.root, "m_modelSkeleton")?, "m_nFlag")?;
    for (j, n) in skel.names.iter().enumerate() {
        if targets.contains(n) {
            flags[j] = i((flags[j].integer()? as i64) & !0x400008);
        }
    }
    let newskel = changed
        .root
        .get_mut("m_modelSkeleton")
        .ok_or("Missing skeleton")?;
    set(newskel, "m_nFlag", list(flags))?;
    unchanged_except(
        field(&before.root, "m_modelSkeleton")?,
        newskel,
        &["m_nFlag"],
    )?;
    let mut skeletons = arr(&before.root, "m_vecNmSkeletonRefs")?;
    require(
        skeletons.iter().all(|v| v.string().ok() != Some(SECONDARY)),
        "Test is already installed; restore first",
    )?;
    if secondary {
        skeletons.push(resource(SECONDARY));
        set(&mut changed.root, "m_vecNmSkeletonRefs", list(skeletons))?;
    }
    unchanged_except(
        &before.root,
        &changed.root,
        &["m_modelSkeleton", "m_vecNmSkeletonRefs"],
    )?;
    let mut edits = BTreeMap::from([
        ("DATA".into(), encode(&changed)?),
        ("PHYS".into(), encode(&phys)?),
    ]);
    if secondary {
        edits.insert("RERL".into(), add_ref(raw, SECONDARY)?);
    }
    let out = rebuild(raw, edits)?;
    let receipt = serde_json::json!({"physics_outputs_disabled":targets,"removed_output_records":removed,"proxy_constraints_unchanged":true,"mesh_payloads_unchanged":true,"input_sha256":hash(raw),"output_sha256":hash(&out)});
    Ok((out, skel, receipt))
}
pub fn prepare_skeleton(original: &[u8], model: &Skeleton) -> Result<(Vec<u8>, Vec<u8>)> {
    let doc = data(original, "DATA")?;
    let mut primary = doc.clone();
    let allowed = [
        "m_ID",
        "m_boneIDs",
        "m_parentIndices",
        "m_parentSpaceReferencePose",
        "m_modelSpaceReferencePose",
        "m_numBonesToSampleAtLowLOD",
        "m_maskDefinitions",
        "m_secondarySkeletons",
        "m_bIsPropSkeleton",
        "m_floatChannelSets",
        "m_gameplayRelevantBoneIndices",
        "m_nSpecialDependencyHash",
    ];
    require(
        doc.root
            .object()?
            .iter()
            .all(|(k, _)| allowed.contains(&k.as_str())),
        "Current game skeleton has fields this experiment has not reviewed",
    )?;
    require(
        arr(&doc.root, "m_boneIDs")?
            .iter()
            .any(|v| v.string().ok() == Some(ANCHOR)),
        "Haze's native skeleton lacks the streamer anchor",
    )?;
    let mut secondaries = arr(&doc.root, "m_secondarySkeletons")?;
    secondaries.push(obj(vec![
        ("m_attachToBoneID", s(ANCHOR)),
        ("m_skeleton", resource(SECONDARY)),
    ]));
    set(&mut primary.root, "m_secondarySkeletons", list(secondaries))?;
    unchanged_except(&doc.root, &primary.root, &["m_secondarySkeletons"])?;
    let primary_raw = rebuild(
        original,
        BTreeMap::from([
            ("DATA".into(), encode(&primary)?),
            ("RERL".into(), add_ref(original, SECONDARY)?),
        ]),
    )?;
    let selected = selected(model)?;
    let mut locals = vec![];
    let mut globals = vec![];
    let (mut p, mut q, mut scale) = ([0.; 3], [0., 0., 0., 1.], 1.0);
    for &index in &selected {
        let t = model.positions[index];
        let r = normalize(model.rotations[index]);
        locals.push(vector(&[
            t[0],
            t[1],
            t[2],
            model.scales[index],
            r[0],
            r[1],
            r[2],
            r[3],
        ]));
        let d = rotate(q, t);
        for j in 0..3 {
            p[j] += scale * d[j];
        }
        q = multiply(q, r);
        scale *= model.scales[index];
        globals.push(vector(&[p[0], p[1], p[2], scale, q[0], q[1], q[2], q[3]]));
    }
    let root = obj(vec![
        ("m_ID", s(SECONDARY)),
        ("m_boneIDs", list(names().iter().map(|n| s(n)).collect())),
        ("m_parentIndices", list((-1..6).map(i).collect())),
        ("m_parentSpaceReferencePose", list(locals)),
        ("m_modelSpaceReferencePose", list(globals)),
        ("m_numBonesToSampleAtLowLOD", i(7)),
        ("m_maskDefinitions", list(vec![])),
        ("m_secondarySkeletons", list(vec![])),
        ("m_floatChannelSets", list(vec![])),
        ("m_bIsPropSkeleton", b(true)),
        ("m_gameplayRelevantBoneIndices", list(vec![])),
    ]);
    let secondary_doc = binary::Document {
        version: 1,
        format_guid: doc.format_guid,
        root,
    };
    let version = container::layout(original)?.resource_version;
    let secondary_raw = envelope(version, &[("DATA".into(), encode(&secondary_doc)?)])?;
    Ok((primary_raw, secondary_raw))
}
fn range(start: f64) -> Value {
    obj(vec![
        ("m_flRangeStart", f(start)),
        ("m_flRangeLength", f(0.1)),
    ])
}
pub fn secondary_clip(
    primary: &Value,
    skel: &Skeleton,
    motion: &Motion,
    animated: bool,
) -> Result<Value> {
    let frames = usize::try_from(field(primary, "m_nNumFrames")?.integer()?)?;
    require(
        (2..=20000).contains(&frames),
        "Idle clip has no bounded timeline",
    )?;
    let indices = selected(skel)?;
    let mut tracks = vec![];
    let mut bytes = vec![];
    for &index in &indices {
        let p = skel.positions[index];
        tracks.push(obj(vec![
            ("m_translationRangeX", range(p[0])),
            ("m_translationRangeY", range(p[1])),
            ("m_translationRangeZ", range(p[2])),
            ("m_scaleRange", range(skel.scales[index])),
            (
                "m_constantRotation",
                vector(&normalize(skel.rotations[index])),
            ),
            ("m_bIsRotationStatic", b(!animated)),
            ("m_bIsTranslationStatic", b(true)),
            ("m_bIsScaleStatic", b(true)),
        ]));
    }
    if animated {
        for frame in 0..frames {
            for (j, &index) in indices.iter().enumerate() {
                let q = multiply(
                    skel.rotations[index],
                    motion.sample(frame as f64 / (frames - 1) as f64, j),
                );
                let words = pack(q);
                require(
                    qdistance(q, unpack(words)) < 0.0003,
                    "Native quaternion encoding exceeds error bound",
                )?;
                for word in words {
                    bytes.extend_from_slice(&word.to_le_bytes());
                }
            }
        }
    }
    let identity = vector(&[0., 0., 0., 1., 0., 0., 0., 1.]);
    Ok(obj(vec![
        ("m_skeleton", resource(SECONDARY)),
        ("m_nNumFrames", i(frames as i64)),
        ("m_flDuration", field(primary, "m_flDuration")?.clone()),
        ("m_compressedPoseData", Value::new(Kind::Blob(bytes))),
        ("m_trackCompressionSettings", list(tracks)),
        (
            "m_compressedPoseOffsets",
            list(
                (0..frames)
                    .map(|j| i(if animated { (j * 21) as i64 } else { 0 }))
                    .collect(),
            ),
        ),
        ("m_floatCurveIDs", list(vec![])),
        ("m_floatCurveDefs", list(vec![])),
        ("m_compressedFloatCurveData", list(vec![])),
        ("m_compressedFloatCurveOffsets", list(vec![])),
        ("m_secondaryAnimations", list(vec![])),
        ("m_syncTrack", field(primary, "m_syncTrack")?.clone()),
        (
            "m_rootMotion",
            obj(vec![
                ("m_transforms", list(vec![])),
                ("m_nNumFrames", i(frames as i64)),
                ("m_flAverageLinearVelocity", f(0.)),
                ("m_flAverageAngularVelocityRadians", f(0.)),
                ("m_totalDelta", identity),
            ]),
        ),
        ("m_bIsAdditive", b(false)),
        ("m_modelSpaceSamplingChain", list(vec![])),
        ("m_modelSpaceBoneSamplingIndices", list(vec![])),
        ("m_events", list(vec![])),
    ]))
}
pub fn inject_clip(
    raw: &[u8],
    skel: &Skeleton,
    motion: &Motion,
    animated: bool,
) -> Result<Vec<u8>> {
    let before = data(raw, "DATA")?;
    let mut doc = before.clone();
    require(
        matches!(
            field(&before.root, "m_bIsAdditive")?.kind,
            Kind::Bool(false)
        ),
        "Additive clips are outside the first playback test",
    )?;
    let mut secondaries = arr(&before.root, "m_secondaryAnimations")?;
    require(
        secondaries
            .iter()
            .all(|v| v.get("m_skeleton").and_then(|s| s.string().ok()) != Some(SECONDARY)),
        "Clip already contains test animation",
    )?;
    secondaries.push(secondary_clip(&before.root, skel, motion, animated)?);
    set(&mut doc.root, "m_secondaryAnimations", list(secondaries))?;
    unchanged_except(&before.root, &doc.root, &["m_secondaryAnimations"])?;
    let out = rebuild(
        raw,
        BTreeMap::from([
            ("DATA".into(), encode(&doc)?),
            ("RERL".into(), add_ref(raw, SECONDARY)?),
        ]),
    )?;
    // Existing public validator validates every native pose, including the new
    // secondary. Self-check does not assert game acceptance or old/new equality.
    deadlock_components::motion::validate_clip(
        &out,
        &out,
        &deadlock_core::CancellationToken::default(),
    )?;
    Ok(out)
}
