use deadlock_playback_test::{native::Result, *};
use std::path::Path;
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let command = args.first().and_then(|s| s.to_str()).unwrap_or("");
    let result=match(command,args.len()){
        ("probe",2)=>probe(Path::new(&args[1]))?,
        ("scan-addons",2)=>scan_addons(Path::new(&args[1]))?,
        ("inspect-model",2)=>inspect_model(Path::new(&args[1]))?,
        ("verify-input",3)=>verify_input(Path::new(&args[1]),Path::new(&args[2]))?,
        ("install",4)=>install::install(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]))?,
        ("restore",2)=>install::restore(Path::new(&args[1]))?,
        ("build",6)=>build(Path::new(&args[1]),Path::new(&args[2]),Path::new(&args[3]),Path::new(&args[4]),Path::new(&args[5]))?,
        _=>return Err("Usage: deadlock-playback-test probe MOD.vpk | inspect-model MODEL.vmdl_c | build GAME_DIR.vpk MOD.vpk MOTION.json OUTPUT_DIRECTORY NATIVE_READER".into()),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Test stopped: {e}");
        std::process::exit(1);
    }
}
