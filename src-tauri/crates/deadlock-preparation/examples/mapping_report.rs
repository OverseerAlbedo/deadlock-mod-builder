//! Generate a read-only suggestion report from a source/weighted/target fixture.
use deadlock_preparation::{algorithms::Bone, automap};
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("Pass a rig fixture JSON path");
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let source: Vec<Bone> = serde_json::from_value(value["source"].clone()).unwrap();
    let target: Vec<Bone> = serde_json::from_value(value["target"].clone()).unwrap();
    let weighted: Vec<String> = serde_json::from_value(value["weighted"].clone()).unwrap();
    let suggestions = automap::suggest(&source, &weighted, &target, &Default::default()).unwrap();
    println!("{}", serde_json::to_string_pretty(&suggestions).unwrap());
}
