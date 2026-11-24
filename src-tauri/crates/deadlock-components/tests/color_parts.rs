//! "This part only": the part rule (`deadlock_project::color_parts`) on made-up
//! meshes, and the shared fixture `tests/fixtures/color-parts.json` that
//! `src/extras/colorParts.ts` must match bit for bit.
use deadlock_project::color_parts::{self, PartMesh};
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/color-parts.json");

fn quad(corners: [[f32; 3]; 4], uvs: [[f32; 2]; 4], base: u32) -> (Vec<f32>, Vec<f32>, Vec<u32>) {
    (corners.concat(), uvs.concat(), vec![base, base + 1, base + 2, base, base + 2, base + 3])
}
/// Two meshes of one material: two quads joined only through equal corner
/// positions (a texture seam), an island, a mirrored copy of the island that
/// shows the same pixels, a triangle reaching past u = 1, a triangle without
/// area and one with a corner index out of range; the second mesh has a
/// triangle that shares a corner position with the first mesh's second quad.
fn meshes() -> Vec<PartMesh> {
    let mut a = PartMesh::default();
    let mut add = |(p, u, i): (Vec<f32>, Vec<f32>, Vec<u32>)| {
        a.positions.extend(p);
        a.uvs.extend(u);
        a.indices.extend(i);
    };
    add(quad([[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]], [[0.1, 0.1], [0.4, 0.1], [0.4, 0.4], [0.1, 0.4]], 0));
    add(quad([[1., 0., 0.], [2., 0., 0.], [2., 1., 0.], [1., 1., 0.]], [[0.6, 0.1], [0.9, 0.1], [0.9, 0.4], [0.6, 0.4]], 4));
    add(quad([[5., 5., 5.], [6., 5., 5.], [6., 6., 5.], [5., 6., 5.]], [[0.1, 0.6], [0.4, 0.6], [0.4, 0.9], [0.1, 0.9]], 8));
    add(quad([[-5., 5., 5.], [-6., 5., 5.], [-6., 6., 5.], [-5., 6., 5.]], [[0.1, 0.6], [0.4, 0.6], [0.4, 0.9], [0.1, 0.9]], 12));
    a.positions.extend([9., 0., 0., 9.5, 0., 0., 9., 0.5, 0.]);
    a.uvs.extend([0.9, 0.6, 1.2, 0.6, 0.9, 0.9]);
    a.indices.extend([16, 17, 18]);
    a.positions.extend([20., 0., 0., 21., 0., 0., 22., 0., 0.]);
    a.uvs.extend([0.5, 0.5, 0.55, 0.55, 0.6, 0.6]);
    a.indices.extend([19, 20, 21]);
    a.indices.extend([0, 1, 99]);
    let b = PartMesh { positions: vec![2.002, 0.001, 0., 3., 0., 0., 3., 1., 0.], uvs: vec![0.7, 0.45, 0.8, 0.45, 0.8, 0.5], indices: vec![0, 1, 2] };
    vec![a, b]
}
const SPOTS: [(f64, f64); 7] = [(0.25, 0.25), (0.25, 0.75), (0.05, 0.62), (0.5, 0.52), (1.25, -0.75), (0.4004, 0.2), (0.75, 0.47)];
const SIZES: [(usize, usize); 3] = [(16, 16), (8, 4), (1, 1)];

fn rows(mask: &[u8], width: usize) -> Vec<String> {
    mask.chunks(width).map(|row| row.iter().map(|v| if *v == 1 { '1' } else { '0' }).collect()).collect()
}
fn pieces_json(pieces: &[Vec<u32>]) -> Json {
    json!(pieces.iter().map(|list| list.iter().map(|p| if *p == u32::MAX { -1i64 } else { *p as i64 }).collect::<Vec<_>>()).collect::<Vec<_>>())
}
fn document() -> Json {
    let meshes = meshes();
    let pieces = color_parts::pieces(&meshes);
    let spots: Vec<Json> = SPOTS
        .iter()
        .map(|(u, v)| {
            let chosen = color_parts::at_spot(&meshes, &pieces, *u, *v);
            let triangles = color_parts::area(&meshes, &pieces, &chosen);
            let masks: Vec<Json> = SIZES
                .iter()
                .map(|(w, h)| json!({"width": w, "height": h, "rows": rows(&color_parts::mask(&triangles, *w, *h), *w)}))
                .collect();
            json!({"u": u, "v": v, "pieces": chosen.iter().collect::<Vec<_>>(), "triangles": triangles.len(), "masks": masks})
        })
        .collect();
    json!({
        "about": "The part rule of color ranges (deadlock_project::color_parts, Rust) and src/extras/colorParts.ts must give these pieces and masks. Made-up meshes only. Written by `cargo test -p deadlock-components --test color_parts write_color_parts_fixture -- --ignored`.",
        "meshes": meshes.iter().map(|m| json!({"positions": m.positions, "uvs": m.uvs, "indices": m.indices})).collect::<Vec<_>>(),
        "pieces": pieces_json(&pieces),
        "spots": spots,
    })
}

#[test]
#[ignore = "writes tests/fixtures/color-parts.json"]
fn write_color_parts_fixture() {
    std::fs::write(FIXTURE, serde_json::to_string_pretty(&document()).unwrap() + "\n").unwrap();
}

#[test]
fn the_rule_matches_the_shared_fixture() {
    let saved: Json = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    // The meshes are read back from the file, as the TypeScript test reads them.
    let meshes: Vec<PartMesh> = saved["meshes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| PartMesh {
            positions: serde_json::from_value(m["positions"].clone()).unwrap(),
            uvs: serde_json::from_value(m["uvs"].clone()).unwrap(),
            indices: serde_json::from_value(m["indices"].clone()).unwrap(),
        })
        .collect();
    assert_eq!(meshes, self::meshes());
    assert_eq!(saved, document());
}

#[test]
fn pieces_join_through_welded_corners_and_spots_choose_what_shows_the_same_pixels() {
    let meshes = meshes();
    let pieces = color_parts::pieces(&meshes);
    // The two quads are one piece (welded seam), with the second mesh's triangle (a corner within 1/200).
    assert!(pieces[0][..4].iter().all(|p| *p == pieces[0][0]));
    assert_eq!(pieces[1][0], pieces[0][0]);
    // Island and mirror: two pieces; out-of-range corner: none.
    assert_ne!(pieces[0][4], pieces[0][6]);
    assert_eq!(*pieces[0].last().unwrap(), u32::MAX);
    let at = |u, v| color_parts::at_spot(&meshes, &pieces, u, v);
    assert_eq!(at(0.25, 0.25), BTreeSet::from([pieces[0][0]]));
    assert_eq!(at(0.25, 0.75), BTreeSet::from([pieces[0][4], pieces[0][6]]), "a mirrored part shares its pixels");
    assert_eq!(at(0.05, 0.62), BTreeSet::from([pieces[0][8]]), "texture coordinates repeat");
    assert_eq!(at(1.25, -0.75), at(0.25, 0.25));
    assert_eq!(at(0.4004, 0.2), at(0.25, 0.25), "within SPOT_REACH of an edge");
    assert!(at(0.5, 0.52).is_empty(), "a triangle without area holds nothing");
    // The mask covers the quads' pixels and a pixel beyond the edge, not the island.
    let area = color_parts::area(&meshes, &pieces, &at(0.25, 0.25));
    let mask = color_parts::mask(&area, 16, 16);
    let px = |x: usize, y: usize| mask[y * 16 + x];
    assert_eq!(px(4, 4), 1);
    assert_eq!(px(1, 2), 1, "one pixel of reach beyond the edge (u = 0.1 is pixel 1.6)");
    assert_eq!(px(4, 12), 0, "the island");
    assert_eq!(px(7, 4), 0, "between the quads");
    // A range for a part reaches nothing without a piece at its spot.
    let range = deadlock_project::color_ranges::ColorRange { hue_shift: 30., part: Some(deadlock_project::color_ranges::ColorPart { u: 0.5, v: 0.52 }), ..Default::default() };
    let error = color_parts::areas(&[range], &meshes).unwrap_err();
    assert!(error.contains("Pick the part again"), "{error}");
    let found = color_parts::areas(&[deadlock_project::color_ranges::ColorRange { hue_shift: 30., ..Default::default() }, deadlock_project::color_ranges::ColorRange { part: Some(deadlock_project::color_ranges::ColorPart { u: 0.25, v: 0.75 }), ..range }], &meshes).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].range, found[0].pieces, found[0].triangles.len()), (1, 2, 4));
    assert!(color_parts::areas(&[deadlock_project::color_ranges::ColorRange::default()], &[]).unwrap().is_empty(), "no part range: nothing is read");
}
