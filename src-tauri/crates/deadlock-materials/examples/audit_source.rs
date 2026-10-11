//! Read-only parity probe for a local prepared source package.
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Supply source package directory")?,
    );
    let summary = deadlock_dmx::Dmx::load(&root.join("Collection.dmx"))?.summary()?;
    let source = root.join("source_materials");
    let audit = deadlock_materials::validate_materials(
        &summary,
        &source,
        Some(&root.join("material_mapping.json")),
        Some(&root.join("material_remap.json")),
    )?;
    let plans = deadlock_materials::plan(
        &summary.materials,
        &source,
        &source,
        &deadlock_materials::Plans::new(),
        &deadlock_core::CancellationToken::new(),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"audit":audit,"plans":plans}))?
    );
    Ok(())
}
