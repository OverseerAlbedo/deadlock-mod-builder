//! Effect edits made before per-part settings existed must keep building to
//! the same bytes. The digests below were taken from `TreeEditExt::edit_part`
//! before the per-part map was added to `TreeEdit`; a change that moves one
//! of them changes what an existing project builds.
//!
//! The synthetic parts cover the cases a per-part setting could disturb: a 3D
//! model with an existing literal size and a plain opacity (the effect-wide
//! sliders already multiply those), a 3D model without either key or any
//! color (the effect-wide sliders leave it alone), sprites, a rope and a
//! trail without the keys (they get them added), and a light with a color
//! scale. `the_game_files_build_as_before` does the same over two heroes'
//! real effect files (`DMB_PAK`, ignored).
use deadlock_components::effect_colors::{ColorReplacement, TreeEdit, TreeEditExt};
use deadlock_resource::{Annotation, Kind, Value, binary, container};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

fn v(kind: Kind) -> Value {
    Value::new(kind)
}
fn s(t: &str) -> Value {
    v(Kind::String(t.into()))
}
fn f(n: f64) -> Value {
    v(Kind::F64(n))
}
fn i(n: i64) -> Value {
    v(Kind::I64(n))
}
fn color(c: &[i64]) -> Value {
    v(Kind::Array(c.iter().map(|n| i(*n)).collect()))
}
fn obj(rows: Vec<(&str, Value)>) -> Value {
    v(Kind::Object(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
}
fn arr(rows: Vec<Value>) -> Value {
    v(Kind::Array(rows))
}
fn literal(n: f64) -> Value {
    obj(vec![("m_nType", s("PF_TYPE_LITERAL")), ("m_flLiteralValue", f(n))])
}
fn resource(root: &Value) -> Vec<u8> {
    let base = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/particle.bin")).unwrap();
    let layout = container::layout(&base).unwrap();
    let block = container::unique(&layout.blocks, "DATA").unwrap();
    let doc = binary::decode(container::payload(&base, block).unwrap()).unwrap();
    container::rebuild(&base, &BTreeMap::from([("DATA".to_owned(), binary::encode(root, doc.format_guid).unwrap())])).unwrap()
}
fn system(rows: Vec<(&str, Value)>) -> Vec<u8> {
    let mut all = vec![("_class", s("CParticleSystemDefinition")), ("m_nMaxParticles", i(8))];
    all.extend(rows);
    resource(&obj(all))
}
fn sha(raw: &[u8]) -> String {
    Sha256::digest(raw).iter().map(|b| format!("{b:02x}")).collect()
}

/// The synthetic parts: (path, file).
fn parts() -> Vec<(&'static str, Vec<u8>)> {
    let mut child = s("particles/sample/sample_glow.vpcf");
    child.annotation = Annotation::Resource;
    vec![
        (
            // A 3D model with a color of its own and both keys set.
            "particles/sample/sample_model_keys.vpcf_c",
            system(vec![
                ("m_ConstantColor", color(&[240, 173, 128, 255])),
                ("m_Initializers", arr(vec![obj(vec![("_class", s("C_INIT_RandomColor")), ("m_ColorMin", color(&[231, 164, 164])), ("m_ColorMax", color(&[186, 143, 129]))])])),
                (
                    "m_Renderers",
                    arr(vec![obj(vec![
                        ("_class", s("C_OP_RenderModels")),
                        ("m_flRadiusScale", literal(2.)),
                        ("m_flAlphaScale", v(Kind::F32(0.5))),
                        ("m_bSuppressTint", v(Kind::Bool(true))),
                    ])]),
                ),
                ("m_Children", arr(vec![obj(vec![("m_ChildRef", child)])])),
            ]),
        ),
        (
            // A 3D model with no color and neither key.
            "particles/sample/sample_model_plain.vpcf_c",
            system(vec![("m_Renderers", arr(vec![obj(vec![("_class", s("C_OP_RenderModels")), ("m_hOverrideMaterial", s("materials/dev/sample.vmat"))])]))]),
        ),
        (
            // Additive sprites with a gradient and a fade.
            "particles/sample/sample_glow.vpcf_c",
            system(vec![
                ("m_ConstantColor", color(&[20, 200, 180, 255])),
                ("m_Operators", arr(vec![
                    obj(vec![("_class", s("C_OP_ColorInterpolate")), ("m_ColorFade", color(&[0, 0, 255, 128]))]),
                    obj(vec![
                        ("_class", s("C_OP_SetVec")),
                        (
                            "m_InputValue",
                            obj(vec![
                                ("m_nType", s("PVEC_TYPE_FLOAT_INTERP_GRADIENT")),
                                ("m_Gradient", obj(vec![("m_Stops", arr(vec![obj(vec![("m_flPosition", f(0.)), ("m_Color", color(&[249, 174, 117]))]), obj(vec![("m_flPosition", f(1.)), ("m_Color", color(&[156, 63, 16]))])]))])),
                            ]),
                        ),
                    ]),
                ])),
                ("m_Renderers", arr(vec![obj(vec![("_class", s("C_OP_RenderSprites")), ("m_nOutputBlendMode", s("PARTICLE_OUTPUT_BLEND_MODE_ADD")), ("m_flOverbrightFactor", f(3.))])])),
            ]),
        ),
        (
            // A rope and a trail without size, opacity or brightness keys.
            "particles/sample/sample_line.vpcf_c",
            system(vec![
                ("m_ConstantColor", color(&[245, 166, 22, 255])),
                ("m_Renderers", arr(vec![obj(vec![("_class", s("C_OP_RenderRopes"))]), obj(vec![("_class", s("C_OP_RenderTrails")), ("m_flRadiusScale", literal(0.5))])])),
            ]),
        ),
        (
            // A light with a color scale and an intensity.
            "particles/sample/sample_light.vpcf_c",
            system(vec![(
                "m_Renderers",
                arr(vec![obj(vec![
                    ("_class", s("C_OP_RenderStandardLight")),
                    ("m_vecColorScale", obj(vec![("m_nType", s("PVEC_TYPE_LITERAL_COLOR")), ("m_LiteralColor", color(&[229, 140, 63]))])),
                    ("m_flIntensity", literal(0.5)),
                ])]),
            )]),
        ),
    ]
}

/// The edits: (name, edit, parts outside the hero's folders).
fn edits() -> Vec<(&'static str, TreeEdit, &'static [&'static str])> {
    vec![
        ("identity", TreeEdit::default(), &[]),
        ("hue", TreeEdit { hue: 115., ..Default::default() }, &[]),
        ("hue_size", TreeEdit { hue: 115., size: 3., ..Default::default() }, &[]),
        ("multipliers", TreeEdit { size: 0.5, glow: 2., opacity: 0.25, ..Default::default() }, &[]),
        (
            "colors",
            TreeEdit {
                saturation: 1.5,
                brightness: 0.8,
                colors: vec![ColorReplacement { from: [249, 174, 117], to: [150, 40, 255], near: true }, ColorReplacement { from: [245, 166, 22], to: [0, 255, 40], near: false }],
                ..Default::default()
            },
            &[],
        ),
        ("excluded", TreeEdit { hue: 90., size: 2., excluded: vec!["PARTICLES/sample/sample_glow.vpcf_c".into()], ..Default::default() }, &[]),
        ("shared_left_out", TreeEdit { hue: 90., opacity: 0.5, ..Default::default() }, &["particles/sample/sample_line.vpcf_c"]),
        ("shared_chosen", TreeEdit { hue: 90., opacity: 0.5, shared: vec!["particles/sample/sample_line.vpcf_c".into()], ..Default::default() }, &["particles/sample/sample_line.vpcf_c"]),
    ]
}

/// Taken before per-part settings existed: one digest per edit over every
/// part's written bytes (path, then the file's SHA-256), in `parts()` order.
const DIGESTS: [(&str, &str); 8] = [
    ("identity", "4b6367a29137d2ba831583f73ffb09f3945113fe0f964e1f88996b3c3b336b59"),
    ("hue", "216e7342d121d48216c4c6421b14858381085fee394c6c09f4135542d24acffa"),
    ("hue_size", "bbfee2786f094b4e1d44d9e06234780999c4f208fc1edd913443c8b6e0d7a579"),
    ("multipliers", "446fd05d5ca169ab44e28f6df6d9e3dbaf8e74f7f66cf47b3fb408215ad2bd12"),
    ("colors", "d6a9d82b2ee40101d131add1db74c29e3a08d9aa8fbf9e2b573dfb56577b5c11"),
    ("excluded", "a01e8b61449c3bb0a9b370a4ad85d477b148a6ea6a69b8a037421936291c8a16"),
    ("shared_left_out", "123b8813dfe94eb72d0351d3e5dbf6398ed87246439f128bbf8a138be1c8cefb"),
    ("shared_chosen", "720ab751fb7c05de5890b84efdf58fc2aa690bb6defa7037d2a9ee133da6b70b"),
];

fn digest(edit: &TreeEdit, outside: &[&str], parts: &[(&str, Vec<u8>)]) -> String {
    let mut all = Sha256::new();
    for (path, raw) in parts {
        let edited = edit.edit_part(raw, path, outside.contains(path)).unwrap();
        all.update(path.as_bytes());
        all.update(sha(&edited.bytes).as_bytes());
    }
    all.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn edits_made_before_per_part_settings_build_the_same_bytes() {
    let parts = parts();
    let mut found = vec![];
    for (name, edit, outside) in edits() {
        found.push((name, digest(&edit, outside, &parts)));
    }
    for (name, d) in &found {
        eprintln!("(\"{name}\", \"{d}\"),");
    }
    for ((name, d), (expected_name, expected)) in found.iter().zip(DIGESTS) {
        assert_eq!(*name, expected_name);
        assert_eq!(d, expected, "{name}: the written bytes changed");
    }
}

/// A saved row without per-part settings serializes exactly as before (the
/// build's receipt and the package's project comparison read it).
#[test]
fn rows_without_per_part_settings_serialize_as_before() {
    let plain = serde_json::to_string(&TreeEdit::default()).unwrap();
    assert_eq!(plain, r#"{"hue":0.0,"saturation":1.0,"brightness":1.0,"excluded":[],"colors":[],"size":1.0,"glow":1.0,"opacity":1.0}"#);
    let full = TreeEdit {
        hue: 115.,
        excluded: vec!["particles/a.vpcf_c".into()],
        shared: vec!["particles/b.vpcf_c".into()],
        colors: vec![ColorReplacement { from: [1, 2, 3], to: [4, 5, 6], near: true }],
        size: 3.,
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&full).unwrap(),
        r#"{"hue":115.0,"saturation":1.0,"brightness":1.0,"excluded":["particles/a.vpcf_c"],"shared":["particles/b.vpcf_c"],"colors":[{"from":[1,2,3],"to":[4,5,6],"near":true}],"size":3.0,"glow":1.0,"opacity":1.0}"#
    );
}

/// Two heroes' real effect files (every `.vpcf_c` under their ability and
/// weapon folders) under three edits, as one digest per hero and edit. The
/// digests hold for the game files they were taken from: after a game update
/// take them again with the code before the change being checked.
#[test]
#[ignore = "needs the game's pak01_dir.vpk in DMB_PAK"]
fn the_game_files_build_as_before() {
    let Ok(pak) = std::env::var("DMB_PAK") else { return };
    let archive = deadlock_vpk::Vpk::open(pak).unwrap();
    const EXPECTED: [(&str, &str, &str); 6] = [
        ("haze", "hue_size", "ba8fb50e13e495c25f557c689afb9ce0ba23e6aba746091a1f2afa34ae0b2439"),
        ("haze", "multipliers", "310c9a3dfa9da7ed3079fdfac8196535a6568ba8ecef89bc2a299c7e010237d4"),
        ("haze", "colors", "f0dad9c448170c964a7b4603895171052715b5d3b266d24ba56471e2c4d79db1"),
        ("ghost", "hue_size", "187d7974a7567cd57667763cf8282d5f90662556f6df10fc701c6566db7e21c6"),
        ("ghost", "multipliers", "cf419ab923c717fdfe01a9a9c886c1d2075797b9198656d5554e7d2c4c3f9575"),
        ("ghost", "colors", "3020807c010c814a36829d0a5676ea718451a6cee31a4137f85ad97d453d6324"),
    ];
    let edits = edits();
    let mut wrong = vec![];
    for (hero, name, expected) in EXPECTED {
        let prefixes = [format!("particles/abilities/{hero}/"), format!("particles/weapon_fx/{hero}/")];
        let paths: Vec<&String> = archive.entries.keys().filter(|p| p.ends_with(".vpcf_c") && prefixes.iter().any(|x| p.starts_with(x.as_str()))).collect();
        let parts: Vec<(&str, Vec<u8>)> = paths.iter().map(|p| (p.as_str(), archive.read(p).unwrap())).collect();
        let edit = &edits.iter().find(|e| e.0 == name).unwrap().1;
        let found = digest(edit, &[], &parts);
        eprintln!("(\"{hero}\", \"{name}\", \"{found}\"), // {} files", parts.len());
        if found != expected {
            wrong.push(format!("{hero} {name}"));
        }
    }
    assert!(wrong.is_empty(), "the written bytes changed: {wrong:?}");
}
