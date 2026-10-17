//! Development aid: list a VPK's files matching a filter, or extract one.
//!
//!   vpk_extract PAK_dir.vpk FILTER            (list)
//!   vpk_extract PAK_dir.vpk PATH OUTPUT_FILE  (extract)
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vpk = deadlock_vpk::Vpk::open(&args[0]).unwrap();
    if let Some(out) = args.get(2) {
        std::fs::write(out, vpk.read(&args[1]).unwrap()).unwrap();
        println!("wrote {out}");
        return;
    }
    for (path, entry) in &vpk.entries {
        if path.contains(args[1].as_str()) {
            println!("{path} {}", entry.length as usize + entry.preload.len());
        }
    }
}
