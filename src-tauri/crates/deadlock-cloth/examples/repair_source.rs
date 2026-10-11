//! Developer reproduction using explicit original/source authority; no install.
use deadlock_skeleton::extensions::SourceBinding;
use std::{fs, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("Usage: repair_source ORIGINAL.vmdl_c CANDIDATE.vmdl_c SOURCE.json REPORT.json OUTPUT.vmdl_c".into());
    }
    let original = fs::read(&args[1])?;
    let candidate = fs::read(&args[2])?;
    let source: SourceBinding = serde_json::from_slice(&fs::read(&args[3])?)?;
    let (output, receipt) = deadlock_cloth::restore(&original, &candidate, Some(&source))?;
    let audit = deadlock_cloth::validate_motion(&original, &output, Some(&source))?;
    if !audit.passed {
        return Err(format!("Motion validation failed: {:?}", audit.errors).into());
    }
    let report = serde_json::json!({"receipt":receipt,"audit":audit,"native_compiler_executed":false,"in_game_tested":false});
    for path in [&args[4], &args[5]] {
        if Path::new(path).exists() {
            return Err("Refusing to overwrite reproduction output".into());
        }
    }
    fs::write(&args[4], serde_json::to_vec_pretty(&report)?)?;
    fs::write(&args[5], output)?;
    println!(
        "Source-qualified bind and names-only repair passed; report: {}",
        args[4]
    );
    Ok(())
}
