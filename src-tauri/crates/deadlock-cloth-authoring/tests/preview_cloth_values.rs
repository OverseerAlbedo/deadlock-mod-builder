//! The movement preview's per-node values (`preview_cloth::node_values` and
//! `effective_tuning`) against the fixture its TypeScript port also reads
//! (`src/preview/clothPacket.test.ts`). Regenerate with
//! `cargo test -p deadlock-cloth-authoring --test preview_cloth_values -- --ignored`.
use deadlock_cloth_authoring::preview_cloth::{effective_tuning, kind_key, node_values, PointMotion};
use deadlock_project::customization::MotionTuning;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const FIXTURE: &str = "tests/fixtures/preview-cloth-node-values.json";

fn cases() -> Vec<Value> {
    let tuning = json!({
        "hair": {"stiffness": -0.4, "swing": 0.8},
        "ear": {"stiffness": 0.8, "swing": -0.6},
        "cloth": {"gravity": -0.5, "damping": 0.4, "swing": 1.0},
        "streamer": {"stiffness": 0.3, "swing": -0.5},
        "breast": {"stiffness": -1.0, "damping": 1.0, "gravity": 1.0, "swing": 1.0},
        "tail": {"stiffness": 0.0, "swing": 0.0}
    });
    let motions = [
        json!({"kind": "hair", "goal": 1.0, "drag": 0.0, "gravity": 0.0, "travel": 0.03, "caps": [{"cap": "held"}]}),
        json!({"kind": "hair", "goal": 0.9, "drag": 0.0, "gravity": 0.3333333333333333, "travel": 2.4, "caps": [{"cap": "at_most", "inches": 1.6}]}),
        json!({"kind": "hair", "goal": 0.45, "drag": 0.0, "gravity": 0.3333333333333333, "travel": 1.1}),
        json!({"kind": "ear", "goal": 1.0, "drag": 0.0, "gravity": 0.0, "travel": 0.03, "caps": [{"cap": "held"}]}),
        json!({"kind": "ear", "goal": 0.85, "drag": 0.0, "gravity": 0.0, "travel": 0.0}),
        json!({"kind": "cloth", "goal": 0.35, "drag": 0.2, "gravity": 1.0, "travel": 3.25}),
        json!({"kind": "streamer", "goal": 0.2, "drag": 0.15, "gravity": 1.0, "travel": 0.0}),
        json!({"kind": "breast", "goal": 0.14, "drag": 0.24, "gravity": 0.0, "travel": 0.9, "caps": [{"cap": "at_most", "inches": 1.2}]}),
        json!({"kind": "tail", "goal": 0.05, "drag": 0.3, "gravity": 0.5, "travel": 4.0}),
        json!({"kind": "charm", "goal": 0.4, "drag": 0.1, "gravity": 1.0, "travel": 0.0}),
        json!({"kind": null, "goal": 0.5, "drag": 0.1, "gravity": 1.0, "travel": 0.5}),
        json!({"kind": "cloth", "goal": 0.3, "drag": 0.1, "gravity": 1.0, "travel": 0.0}),
        json!({"kind": "cloth", "goal": 0.3, "drag": 0.1, "gravity": 1.0, "travel": 2.0}),
        json!({"kind": "hair", "goal": 0.6, "drag": 0.0, "gravity": 0.0, "travel": 0.02}),
    ];
    let attachments = [None, None, None, None, None, None, None, None, None, None, None, Some(0.4), Some(3.0), None];
    let mut out = vec![];
    for with_tuning in [false, true] {
        let t = if with_tuning { tuning.clone() } else { json!({}) };
        let map: BTreeMap<String, MotionTuning> = serde_json::from_value(t.clone()).unwrap();
        let effective = effective_tuning(&map);
        for (m, a) in motions.iter().zip(attachments) {
            let motion: PointMotion = serde_json::from_value(m.clone()).unwrap();
            let kind = motion.kind.map(kind_key);
            let v = node_values(&motion, kind.as_ref().and_then(|k| effective.get(k)), a);
            out.push(json!({"motion": m, "attachment": a, "tuning": t, "expected": v}));
        }
    }
    out
}

#[test]
#[ignore = "writes the shared fixture"]
fn write_node_values_fixture() {
    std::fs::write(FIXTURE, serde_json::to_string_pretty(&cases()).unwrap() + "\n").unwrap();
}

#[test]
fn node_values_match_the_shared_fixture() {
    let saved: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    let now = cases();
    assert_eq!(saved.len(), now.len());
    for (a, b) in saved.iter().zip(&now) {
        assert_eq!(a["motion"], b["motion"]);
        for key in ["attraction", "damping", "gravity", "travel"] {
            let (x, y) = (a["expected"][key].as_f64().unwrap(), b["expected"][key].as_f64().unwrap());
            assert!((x - y).abs() < 1e-9, "{key}: {x} against {y} for {}", a["motion"]);
        }
    }
}
