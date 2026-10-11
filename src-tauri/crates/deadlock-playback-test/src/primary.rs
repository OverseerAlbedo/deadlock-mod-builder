//! Direct-primary attachment experiment. Preserve all existing bone indices;
//! every clip using this skeleton must gain seven matching tracks before install.
use crate::{animation::*, native::*};
use deadlock_resource::{Kind, Value};
use deadlock_skeleton::Skeleton;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct Binding {
    pub skeleton: String,
    pub original_bones: usize,
    pub indices: Vec<usize>,
}

fn boolean(v: &Value) -> Result<bool> {
    match v.kind {
        Kind::Bool(x) => Ok(x),
        _ => Err("Expected native boolean".into()),
    }
}
fn transform(v: &Value) -> Result<[f64; 8]> {
    let a = v.array()?;
    require(a.len() == 8, "Invalid native reference transform")?;
    let mut out = [0.; 8];
    for (j, value) in a.iter().enumerate() {
        out[j] = number(value)?;
    }
    require(
        out.iter().all(|x| x.is_finite()),
        "Non-finite reference transform",
    )?;
    Ok(out)
}

pub fn skeleton(raw: &[u8], model: &Skeleton) -> Result<(Vec<u8>, Binding)> {
    let before = data(raw, "DATA")?;
    let mut doc = before.clone();
    let id = field(&before.root, "m_ID")?.string()?.to_owned();
    require(
        !boolean(field(&before.root, "m_bIsPropSkeleton")?)?,
        "Primary skeleton is a prop",
    )?;
    let mut bones = arr(&before.root, "m_boneIDs")?;
    let count = bones.len();
    let anchor = bones
        .iter()
        .position(|b| b.string().ok() == Some(ANCHOR))
        .ok_or("No native spine anchor")?;
    let new_names = names();
    require(
        bones
            .iter()
            .all(|v| v.string().is_ok_and(|n| !new_names.iter().any(|x| x == n))),
        "Streamer is already in the primary skeleton; restore the original mod",
    )?;
    let mut parents = arr(&before.root, "m_parentIndices")?;
    let mut locals = arr(&before.root, "m_parentSpaceReferencePose")?;
    let mut globals = arr(&before.root, "m_modelSpaceReferencePose")?;
    require(
        count > 0
            && count < 4096
            && parents.len() == count
            && locals.len() == count
            && globals.len() == count,
        "Native skeleton arrays disagree",
    )?;
    for (j, p) in parents.iter().enumerate() {
        let n = p.integer()?;
        require(
            n >= -1 && n < j as i128,
            "Native skeleton is not parent-before-child",
        )?;
    }
    let indices = selected(model)?;
    let mut parent_pose = transform(&globals[anchor])?;
    for (j, &index) in indices.iter().enumerate() {
        let p = model.positions[index];
        let q = normalize(model.rotations[index]);
        let scale = model.scales[index];
        bones.push(s(&new_names[j]));
        parents.push(i(if j == 0 { anchor } else { count + j - 1 } as i64));
        locals.push(vector(&[p[0], p[1], p[2], scale, q[0], q[1], q[2], q[3]]));
        let pq = [
            parent_pose[4],
            parent_pose[5],
            parent_pose[6],
            parent_pose[7],
        ];
        let rotated = rotate(pq, p);
        let rotation = multiply(pq, q);
        parent_pose = [
            parent_pose[0] + parent_pose[3] * rotated[0],
            parent_pose[1] + parent_pose[3] * rotated[1],
            parent_pose[2] + parent_pose[3] * rotated[2],
            parent_pose[3] * scale,
            rotation[0],
            rotation[1],
            rotation[2],
            rotation[3],
        ];
        globals.push(vector(&parent_pose));
    }
    for (k, values) in [
        ("m_boneIDs", bones),
        ("m_parentIndices", parents),
        ("m_parentSpaceReferencePose", locals),
        ("m_modelSpaceReferencePose", globals),
    ] {
        set(&mut doc.root, k, list(values))?;
    }
    // Keep the original indices used by graph IK/sampling chains. Sampling all
    // bones in this small test avoids inserting into the low-LOD prefix.
    set(
        &mut doc.root,
        "m_numBonesToSampleAtLowLOD",
        i((count + 7) as i64),
    )?;
    let mut masks = arr(&before.root, "m_maskDefinitions")?;
    for mask in &mut masks {
        let weights = mask
            .get_mut("m_primaryWeightList")
            .ok_or("Unknown primary bone mask layout")?;
        require(
            field(weights, "m_skeletonName")?.string()? == id,
            "Mask targets another skeleton",
        )?;
        let mut ids = arr(weights, "m_boneIDs")?;
        let mut values = arr(weights, "m_weights")?;
        require(ids.len() == values.len(), "Mask IDs and weights disagree")?;
        let anchor_weight = ids
            .iter()
            .position(|v| v.string().ok() == Some(ANCHOR))
            .map(|j| values[j].clone())
            .unwrap_or(f(0.));
        let weight = number(&anchor_weight)?;
        require(
            (0.0..=1.0).contains(&weight),
            "Unsupported anchor mask weight",
        )?;
        ids.extend(new_names.iter().map(|n| s(n)));
        values.extend((0..7).map(|_| anchor_weight.clone()));
        set(weights, "m_boneIDs", list(ids))?;
        set(weights, "m_weights", list(values))?;
    }
    set(&mut doc.root, "m_maskDefinitions", list(masks))?;
    unchanged_except(
        &before.root,
        &doc.root,
        &[
            "m_boneIDs",
            "m_parentIndices",
            "m_parentSpaceReferencePose",
            "m_modelSpaceReferencePose",
            "m_numBonesToSampleAtLowLOD",
            "m_maskDefinitions",
        ],
    )?;
    // Resource dependency metadata is retained; this experiment does not claim
    // to reproduce Source 2 resource-compiler dependency hashes.
    Ok((
        rebuild(raw, BTreeMap::from([("DATA".into(), encode(&doc)?)]))?,
        Binding {
            skeleton: id,
            original_bones: count,
            indices,
        },
    ))
}

/// Keep the seven added streamer bones out of the aiming replacement used
/// during turns, so the base turn clip can drive them. Original mask entries
/// and every body bone remain exactly as in the game.
pub fn behavior_skeleton(raw: &[u8], model: &Skeleton) -> Result<(Vec<u8>, Binding)> {
    let (base, binding) = skeleton(raw, model)?;
    let before = data(&base, "DATA")?;
    let mut doc = before.clone();
    let mut masks = arr(&doc.root, "m_maskDefinitions")?;
    let mut found = false;
    for mask in &mut masks {
        if field(mask, "m_ID")?.string()? != "UpperBodyReplace_Turn" {
            continue;
        }
        require(!found, "Duplicate turn replacement mask")?;
        found = true;
        let weights = mask
            .get_mut("m_primaryWeightList")
            .ok_or("Missing turn weights")?;
        let ids = arr(weights, "m_boneIDs")?;
        let mut values = arr(weights, "m_weights")?;
        let selected = names();
        let mut count = 0;
        for (j, id) in ids.iter().enumerate() {
            if selected.iter().any(|n| id.string().ok() == Some(n)) {
                values[j] = f(0.);
                count += 1;
            }
        }
        require(count == 7, "Turn mask lacks the new streamer bones")?;
        set(weights, "m_weights", list(values))?;
    }
    require(
        found,
        "Native turn replacement mask changed; collect results before adapting",
    )?;
    set(&mut doc.root, "m_maskDefinitions", list(masks))?;
    unchanged_except(&before.root, &doc.root, &["m_maskDefinitions"])?;
    Ok((
        rebuild(&base, BTreeMap::from([("DATA".into(), encode(&doc)?)]))?,
        binding,
    ))
}

pub fn has_authored_motion(path: &str, root: &Value) -> Result<bool> {
    let frames = field(root, "m_nNumFrames")?.integer()?;
    let duration = number(field(root, "m_flDuration")?)?;
    Ok(path.to_ascii_lowercase().contains("idle")
        && !boolean(field(root, "m_bIsAdditive")?)?
        && (8..=20000).contains(&frames)
        && (0.5..=60.).contains(&duration))
}

fn track(p: [f64; 3], q: [f64; 4], scale: f64, offset: usize, animated: bool) -> Value {
    let range = |start| {
        obj(vec![
            ("m_flRangeStart", f(start)),
            ("m_flRangeLength", f(0.1)),
        ])
    };
    obj(vec![
        ("m_translationRangeX", range(p[0])),
        ("m_translationRangeY", range(p[1])),
        ("m_translationRangeZ", range(p[2])),
        ("m_scaleRange", range(scale)),
        ("m_nTrackReadOffset", i(offset as i64)),
        ("m_constantRotation", vector(&q)),
        ("m_bIsRotationStatic", b(!animated)),
        ("m_bIsTranslationStatic", b(true)),
        ("m_bIsScaleStatic", b(true)),
    ])
}
fn frame_words(tracks: &[Value]) -> Result<usize> {
    let mut words = 0;
    for t in tracks {
        // Native offsets are REQUIRED here, unlike the older permissive reader.
        require(
            field(t, "m_nTrackReadOffset")?.integer()? == words as i128,
            "Native track read offset does not match its packed pose",
        )?;
        if !boolean(field(t, "m_bIsRotationStatic")?)? {
            words += 3;
        }
        if !boolean(field(t, "m_bIsTranslationStatic")?)? {
            words += 3;
        }
        if !boolean(field(t, "m_bIsScaleStatic")?)? {
            words += 1;
        }
    }
    Ok(words)
}

pub fn clip(
    raw: &[u8],
    binding: &Binding,
    model: &Skeleton,
    motion: &Motion,
    animated: bool,
) -> Result<Vec<u8>> {
    let root = data(raw, "DATA")?;
    let frames = field(&root.root, "m_nNumFrames")?.integer()? as usize;
    require(
        !animated || !boolean(field(&root.root, "m_bIsAdditive")?)? && frames >= 8,
        "Authored sweep requires a nonadditive timeline",
    )?;
    let samples = animated.then(|| {
        (0..frames)
            .map(|f| {
                (0..7)
                    .map(|j| motion.sample(f as f64 / (frames - 1) as f64, j))
                    .collect()
            })
            .collect::<Vec<Vec<_>>>()
    });
    clip_with_deltas(raw, binding, model, samples.as_deref())
}

/// Append authored rotations, including true additive deltas for native stop
/// clips. Every original body sample, timing, event and track remains exact.
pub fn clip_with_deltas(
    raw: &[u8],
    binding: &Binding,
    model: &Skeleton,
    samples: Option<&[Vec<[f64; 4]>]>,
) -> Result<Vec<u8>> {
    let animated = samples.is_some();
    let before = data(raw, "DATA")?;
    let mut doc = before.clone();
    require(
        field(&before.root, "m_skeleton")?.string()? == binding.skeleton,
        "Clip uses a different primary skeleton",
    )?;
    let mut tracks = arr(&before.root, "m_trackCompressionSettings")?;
    require(
        tracks.len() == binding.original_bones,
        "Clip and primary skeleton bone counts disagree",
    )?;
    let original_words = frame_words(&tracks)?;
    let frames = usize::try_from(field(&before.root, "m_nNumFrames")?.integer()?)?;
    let additive = boolean(field(&before.root, "m_bIsAdditive")?)?;
    if let Some(samples) = samples {
        require(
            frames >= 2 && samples.len() == frames && samples.iter().all(|f| f.len() == 7),
            "Authored sample dimensions differ from native clip",
        )?;
        require(
            samples.iter().flatten().all(|q| {
                q.iter().all(|v| v.is_finite())
                    && (q.iter().map(|v| v * v).sum::<f64>() - 1.).abs() < 1e-5
            }),
            "Invalid authored delta",
        )?;
    }
    let Kind::Blob(bytes) = &field(&before.root, "m_compressedPoseData")?.kind else {
        return Err("No native pose blob".into());
    };
    let offsets = arr(&before.root, "m_compressedPoseOffsets")?;
    require(
        (1..=1_000_000).contains(&frames)
            && offsets.len() == frames
            && bytes.len() == 2 * frames * original_words,
        "Unsupported native pose span",
    )?;
    require(
        offsets
            .iter()
            .enumerate()
            .all(|(j, v)| v.integer().ok() == Some((j * original_words) as i128)),
        "Unsupported noncontiguous pose offsets",
    )?;
    for (j, &index) in binding.indices.iter().enumerate() {
        // Native additive tracks store DELTAS: zero translation AND zero scale,
        // identity rotation. A bind pose here would add a second rest transform.
        let (p, q, scale) = if additive {
            ([0.; 3], [0., 0., 0., 1.], 0.)
        } else {
            (
                model.positions[index],
                normalize(model.rotations[index]),
                model.scales[index],
            )
        };
        tracks.push(track(
            p,
            q,
            scale,
            original_words + if animated { j * 3 } else { 0 },
            animated,
        ));
    }
    set(&mut doc.root, "m_trackCompressionSettings", list(tracks))?;
    if animated {
        let new_words = original_words + 21;
        let mut output = Vec::with_capacity(frames * new_words * 2);
        for frame in 0..frames {
            let start = frame * original_words * 2;
            output.extend_from_slice(&bytes[start..start + original_words * 2]);
            for (j, &index) in binding.indices.iter().enumerate() {
                let delta = samples.unwrap()[frame][j];
                let q = if additive {
                    delta
                } else {
                    multiply(model.rotations[index], delta)
                };
                let words = pack(q);
                require(
                    qdistance(q, unpack(words)) < 0.0003,
                    "Authored quaternion exceeds native error tolerance",
                )?;
                for word in words {
                    output.extend_from_slice(&word.to_le_bytes());
                }
            }
            // Verify every original sample against the final interleaved layout.
            let dest = frame * new_words * 2;
            require(
                output[dest..dest + original_words * 2] == bytes[start..start + original_words * 2],
                "Original body sample changed",
            )?;
        }
        set(
            &mut doc.root,
            "m_compressedPoseData",
            Value::new(Kind::Blob(output)),
        )?;
        set(
            &mut doc.root,
            "m_compressedPoseOffsets",
            list((0..frames).map(|j| i((j * new_words) as i64)).collect()),
        )?;
    }
    unchanged_except(
        &before.root,
        &doc.root,
        &[
            "m_trackCompressionSettings",
            "m_compressedPoseData",
            "m_compressedPoseOffsets",
        ],
    )?;
    let old_tracks = arr(&before.root, "m_trackCompressionSettings")?;
    let new_tracks = arr(&doc.root, "m_trackCompressionSettings")?;
    require(
        old_tracks
            .iter()
            .zip(&new_tracks)
            .all(|(a, b)| a.canonical_eq(b)),
        "Original track settings changed",
    )?;
    frame_words(&new_tracks)?;
    let result = rebuild(raw, BTreeMap::from([("DATA".into(), encode(&doc)?)]))?;
    deadlock_components::motion::validate_clip(
        &result,
        &result,
        &deadlock_core::CancellationToken::default(),
    )?;
    Ok(result)
}

/// Gameplay coverage remains strict. UI-only clips already absent from the
/// native archive are reported and left absent, rather than synthesized.
pub fn verify_coverage(
    gameplay: &BTreeSet<String>,
    ui: &BTreeSet<String>,
    available: &BTreeSet<String>,
) -> Result<Vec<String>> {
    for path in gameplay {
        require(
            available.contains(path),
            &format!("Gameplay graph animation is missing from the installed game: {path}"),
        )?;
    }
    Ok(ui
        .difference(available)
        .filter(|p| !gameplay.contains(*p))
        .cloned()
        .collect())
}
