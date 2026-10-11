use deadlock_core::{CancellationToken, TimingRecorder, TimingStatus};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: packet INPUT.dmx OUTPUT.dmbp".into());
    }
    let clock = TimingRecorder::new("DMX preview");
    let packet =
        deadlock_preview::load_packet(Path::new(&args[1]), &CancellationToken::default(), &clock)?;
    std::fs::write(&args[2], &packet)?;
    let report = clock.finish(TimingStatus::Succeeded, None)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
