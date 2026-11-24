//! Color ranges that give what they select a new color (`set_hue`,
//! `set_saturation`, `lift`) and ranges for one part (`part`): the maths, what
//! counts as a change, the saved format's limits, the switches, and that a
//! range without the new fields is exactly what it was. Synthetic values only.
use deadlock_components::color_ranges::{self, ColorPart, ColorRange, Switches};
use deadlock_project::customization::{Customization, HeroColorRange};
use serde_json::{Value as Json, json};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hero-color-ranges.json");

fn grey_to_violet() -> ColorRange {
    ColorRange {
        saturation_range: [0., 0.2],
        brightness_range: [0.55, 1.],
        set_hue: Some(270.),
        set_saturation: Some(0.6),
        ..ColorRange::default()
    }
}

#[test]
fn a_range_without_the_new_fields_serializes_and_recolors_as_before() {
    // The exact JSON receipts and texture fingerprints hold for a range.
    let old = ColorRange { hue: 26., hue_width: 12., saturation_range: [0.6, 1.], hue_shift: 180., ..ColorRange::default() };
    assert_eq!(
        serde_json::to_string(&old).unwrap(),
        r#"{"hue":26.0,"hue_width":12.0,"saturation_range":[0.6,1.0],"brightness_range":[0.0,1.0],"softness":0.25,"hue_shift":180.0,"saturation":1.0,"brightness":1.0}"#
    );
    // A saved range without the fields reads back without them; a lift of 0 is no lift.
    let saved: HeroColorRange = serde_json::from_value(json!({"hue": 26.0, "hue_shift": 180.0, "lift": 0.0})).unwrap();
    let range = ColorRange::from_choice(&saved);
    assert_eq!(range.lift, None);
    assert_eq!(range, ColorRange { hue: 26., hue_shift: 180., ..ColorRange::default() });
    // The masked function with every range reaching is the plain one, texel for texel.
    let ranges = [old, ColorRange { brightness_range: [0., 0.3], softness: 0., brightness: 0.5, ..ColorRange::default() }];
    for r in (0..=255u16).step_by(17) {
        for g in (0..=255u16).step_by(51) {
            for b in (0..=255u16).step_by(85) {
                let texel = [r as u8, g as u8, b as u8, 77];
                assert_eq!(color_ranges::recolor_where(&ranges, texel, |_| true), color_ranges::recolor(&ranges, texel));
            }
        }
    }
    // A range kept out of a texel leaves it to the others.
    assert_eq!(color_ranges::recolor_where(&ranges[..1], [140, 80, 36, 5], |_| false), [140, 80, 36, 5]);
}

#[test]
fn a_given_color_replaces_hue_and_saturation_and_keeps_shading() {
    let range = grey_to_violet();
    for grey in [150u8, 200, 240] {
        let out = color_ranges::recolor(&[range], [grey, grey, grey, 9]);
        let [h, s, v] = color_ranges::hsv(out.map(|c| c as f64 / 255.)[..3].try_into().unwrap());
        assert!((h - 270.).abs() < 2., "violet hue: {out:?}");
        assert!((s - 0.6).abs() < 0.02, "saturation 0.6: {out:?}");
        assert!((v - grey as f64 / 255.).abs() < 0.01, "brightness kept: {out:?}");
        assert_eq!(out[3], 9, "alpha kept");
    }
    // A shift on a grey does nothing at all: that is why grey needs a given color.
    let shifted = ColorRange { hue_shift: 90., set_hue: None, set_saturation: None, ..grey_to_violet() };
    assert_eq!(color_ranges::recolor(&[shifted], [200, 200, 200, 255]), [200, 200, 200, 255]);
    // Black takes a color only with a lift.
    let black = ColorRange { brightness_range: [0., 0.2], set_hue: Some(0.), set_saturation: Some(0.8), ..ColorRange::default() };
    assert_eq!(color_ranges::recolor(&[black], [6, 6, 6, 255]), [6, 1, 1, 255]);
    let lifted = ColorRange { lift: Some(0.25), ..black };
    let out = color_ranges::recolor(&[lifted], [6, 6, 6, 255]);
    assert!(out[0] > 60 && out[1] < 20 && out[2] < 20, "dark red: {out:?}");
    // Brightness still scales first, then the lift is added and clamped.
    let dimmer = ColorRange { brightness: 0.5, lift: Some(-1.), ..grey_to_violet() };
    assert_eq!(color_ranges::recolor(&[dimmer], [220, 220, 220, 255]), [0, 0, 0, 255]);
    // A new hue of 360 is red, like 0.
    let red = ColorRange { set_hue: Some(360.), set_saturation: Some(1.), ..grey_to_violet() };
    assert_eq!(color_ranges::recolor(&[red], [200, 200, 200, 255]), [200, 0, 0, 255]);
}

#[test]
fn ranges_that_give_a_color_always_change_something() {
    let neutral = ColorRange { set_hue: Some(0.), set_saturation: Some(0.), ..ColorRange::default() };
    assert!(!neutral.changes_nothing(), "a grey given a grey is still the user's choice");
    assert!(!ColorRange { lift: Some(0.1), ..ColorRange::default() }.changes_nothing());
    assert!(ColorRange { lift: Some(0.), ..ColorRange::default() }.changes_nothing());
    assert!(ColorRange { part: Some(ColorPart { u: 0.5, v: 0.5 }), ..ColorRange::default() }.changes_nothing(), "a part alone changes nothing");
    let rows: Vec<HeroColorRange> = serde_json::from_value(json!([
        {"saturation_range": [0.0, 0.2], "set_hue": 270.0, "set_saturation": 0.6},
        {"hue_shift": 0.0},
        {"lift": 0.0},
        {"enabled": false, "set_hue": 10.0, "set_saturation": 1.0}
    ]))
    .unwrap();
    let kept = color_ranges::enabled(&rows);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].set_hue, Some(270.));
}

#[test]
fn the_new_values_have_limits_and_come_as_a_pair() {
    assert!(grey_to_violet().validate().is_ok());
    assert_eq!(ColorRange { set_saturation: None, ..grey_to_violet() }.validate().unwrap_err(), color_ranges::GIVEN_COLOUR_PAIR);
    assert!(ColorRange { set_hue: Some(361.), ..grey_to_violet() }.validate().is_err());
    assert!(ColorRange { set_saturation: Some(1.5), ..grey_to_violet() }.validate().is_err());
    assert!(ColorRange { lift: Some(-1.5), ..grey_to_violet() }.validate().is_err());
    assert!(ColorRange { part: Some(ColorPart { u: f64::NAN, v: 0. }), ..grey_to_violet() }.validate().is_err());
    assert!(ColorRange { part: Some(ColorPart { u: -2.5, v: 3. }), ..grey_to_violet() }.validate().is_ok(), "texture coordinates repeat");
    let check = |range: Json| {
        let state: Customization = serde_json::from_value(json!({"hero_materials": {"enabled": true, "materials": [
            {"target": "models/heroes_staging/haze/materials/hazev2.vmat_c", "ranges": [range]}
        ]}}))
        .unwrap();
        state.validate().map_err(|e| e.to_string())
    };
    assert!(check(json!({"set_hue": 270.0, "set_saturation": 0.6, "lift": 0.2, "part": {"u": 0.3, "v": 0.7}})).is_ok());
    assert!(check(json!({"set_hue": 270.0})).unwrap_err().contains("needs both its hue and its saturation"));
    assert!(check(json!({"set_hue": 270.0, "set_saturation": 2.0})).is_err());
    assert!(check(json!({"lift": 1.5})).is_err());
    // Surface changes rows take the same ranges.
    let state: Customization = serde_json::from_value(json!({"appearance": {"enabled": true, "materials": [
        {"target": "m/body.vmat", "ranges": [{"set_hue": 10.0}]}
    ]}}))
    .unwrap();
    assert!(state.validate().is_err());
}

#[test]
fn the_app_s_switches_are_the_enabled_abilities() {
    // Both abilities are enabled in the app.
    assert_eq!(color_ranges::SWITCHES, Switches::ALL);
    assert_eq!(color_ranges::SWITCHES, Switches { set_colour: true, part_only: true });
    assert!(grey_to_violet().check_switches(color_ranges::SWITCHES).is_ok());
    let part = ColorRange { hue_shift: 30., part: Some(ColorPart { u: 0.1, v: 0.2 }), lift: Some(0.1), ..ColorRange::default() };
    assert!(part.check_switches(color_ranges::SWITCHES).is_ok());
}

#[test]
fn a_switch_turned_off_refuses_the_ranges_that_need_it() {
    let off = Switches::NONE;
    assert_eq!(grey_to_violet().check_switches(off).unwrap_err(), color_ranges::SET_COLOUR_OFF);
    let lift = ColorRange { lift: Some(0.2), ..ColorRange::default() };
    assert_eq!(lift.check_switches(off).unwrap_err(), color_ranges::SET_COLOUR_OFF);
    let part = ColorRange { hue_shift: 30., part: Some(ColorPart { u: 0.1, v: 0.2 }), ..ColorRange::default() };
    assert_eq!(part.check_switches(off).unwrap_err(), color_ranges::PART_ONLY_OFF);
    assert!(part.check_switches(Switches { set_colour: true, part_only: false }).is_err());
    assert!(part.check_switches(Switches { set_colour: false, part_only: true }).is_ok());
    assert!(grey_to_violet().check_switches(Switches::ALL).is_ok());
    assert!(ColorRange { hue_shift: 30., ..ColorRange::default() }.check_switches(off).is_ok(), "ordinary ranges never need a switch");
    // The page's switches are the same (src/extras/heroColorRanges.ts COLOR_SWITCHES).
    let page = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../src/extras/heroColorRanges.ts")).unwrap();
    let expected = format!(
        "export const COLOR_SWITCHES: ColorSwitches = {{ set_colour: {}, part_only: {} }};",
        color_ranges::SWITCHES.set_colour,
        color_ranges::SWITCHES.part_only
    );
    assert!(page.contains(&expected), "heroColorRanges.ts must hold: {expected}");
}

#[test]
fn a_project_with_new_ranges_is_saved_for_older_apps_to_refuse() {
    let mut state: Customization = serde_json::from_value(json!({"hero_materials": {"enabled": true, "materials": [
        {"target": "models/heroes_staging/haze/materials/hazev2.vmat_c", "ranges": [{"hue_shift": 30.0}]}
    ]}}))
    .unwrap();
    let before = serde_json::to_string(&state).unwrap();
    state.mark_for_older_apps();
    assert_eq!(serde_json::to_string(&state).unwrap(), before, "ordinary ranges keep their bytes (no schema added)");
    let mut marked: Customization = serde_json::from_value(json!({"schema": 1, "appearance": {"enabled": true, "materials": [
        {"target": "m/body.vmat", "ranges": [{"saturation_range": [0.0, 0.1], "set_hue": 10.0, "set_saturation": 0.5}]}
    ]}}))
    .unwrap();
    marked.mark_for_older_apps();
    assert_eq!(marked.schema, Some(deadlock_project::customization::NEWER_RANGES_SCHEMA));
    assert!(marked.validate().is_ok(), "this app opens it");
    // The check every older app runs when it opens a project (0.13.0's words).
    assert!(marked.schema.is_some_and(|v| v != 1), "0.13.0 refuses: Unsupported character extras version.");
    // Without such a range the mark goes again.
    marked.appearance.as_mut().unwrap().materials.as_mut().unwrap()[0].ranges = Some(vec![]);
    marked.mark_for_older_apps();
    assert_eq!(marked.schema, Some(1));
    let unknown: Customization = serde_json::from_value(json!({"schema": 3})).unwrap();
    assert!(unknown.validate().is_err());
}

// ---- The shared fixture: cases for the new fields, added after the old ones.

fn texels() -> Vec<[u8; 4]> {
    let mut out = vec![];
    for v in [0u8, 6, 15, 40, 90, 128, 150, 200, 230, 255] {
        out.push([v, v, v, 255]);
    }
    out.extend([
        [200, 196, 192, 3],
        [210, 200, 205, 128],
        [8, 4, 10, 255],
        [30, 34, 30, 0],
        [140, 80, 36, 255],
        [61, 111, 214, 255],
        [181, 160, 148, 200],
        [250, 248, 246, 255],
        [100, 101, 120, 255],
    ]);
    out
}
fn new_cases() -> Vec<(&'static str, Vec<ColorRange>)> {
    let black = ColorRange { brightness_range: [0., 0.2], set_hue: Some(0.), set_saturation: Some(0.8), lift: Some(0.25), ..ColorRange::default() };
    vec![
        ("light grey given violet", vec![grey_to_violet()]),
        ("black given dark red with a lift", vec![black]),
        ("white given blue and darker", vec![ColorRange { brightness_range: [0.8, 1.], saturation_range: [0., 0.15], set_hue: Some(220.), set_saturation: Some(0.7), brightness: 0.8, ..ColorRange::default() }]),
        ("a lift alone darkens", vec![ColorRange { saturation_range: [0., 0.3], lift: Some(-0.2), ..ColorRange::default() }]),
        ("a given color after a shift", vec![ColorRange { hue: 25., hue_width: 15., saturation_range: [0.5, 1.], hue_shift: 120., ..ColorRange::default() }, ColorRange { softness: 0., ..grey_to_violet() }]),
        ("a new hue of 360 and a part spot the pixel function ignores", vec![ColorRange { set_hue: Some(360.), set_saturation: Some(1.), part: Some(ColorPart { u: 0.25, v: 0.75 }), ..grey_to_violet() }]),
    ]
}

#[test]
#[ignore = "adds the new-field cases to tests/fixtures/hero-color-ranges.json"]
fn write_new_field_cases() {
    let text = std::fs::read_to_string(FIXTURE).unwrap();
    let saved: Json = serde_json::from_str(&text).unwrap();
    let names: Vec<String> = saved["cases"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_owned()).collect();
    let mut added = String::new();
    for (name, ranges) in new_cases() {
        if names.iter().any(|n| n == name) {
            continue;
        }
        let colors = texels();
        let expected: Vec<[u8; 4]> = colors.iter().map(|c| color_ranges::recolor(&ranges, *c)).collect();
        // The file's own layout: keys name, ranges, colors, expected, two-space pretty print.
        let case = format!(
            "{{\n  \"name\": {},\n  \"ranges\": {},\n  \"colors\": {},\n  \"expected\": {}\n}}",
            serde_json::to_string(name).unwrap(),
            serde_json::to_string_pretty(&ranges).unwrap(),
            serde_json::to_string_pretty(&colors).unwrap(),
            serde_json::to_string_pretty(&expected).unwrap()
        );
        let indented: Vec<String> = case.lines().map(|l| format!("    {l}")).collect();
        added.push_str(",\n");
        added.push_str(&indented.join("\n"));
    }
    if added.is_empty() {
        return;
    }
    let end = text.rfind("\n  ]").expect("the cases array ends the file");
    let mut out = text[..end].to_owned();
    out.push_str(&added);
    out.push_str(&text[end..]);
    serde_json::from_str::<Json>(&out).unwrap();
    std::fs::write(FIXTURE, out).unwrap();
}

#[test]
fn the_fixture_holds_the_new_field_cases() {
    let saved: Json = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    for (name, ranges) in new_cases() {
        let case = saved["cases"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap_or_else(|| panic!("missing case {name}"));
        let read: Vec<ColorRange> = serde_json::from_value(case["ranges"].clone()).unwrap();
        assert_eq!(read, ranges, "{name}");
    }
}
