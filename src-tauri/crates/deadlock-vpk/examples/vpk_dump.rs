//! Development aid: extract every file whose path contains FILTER into OUT_DIR
//! (folders flattened, "/" becomes "__").
//!
//!   vpk_dump PAK_dir.vpk FILTER OUT_DIR
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vpk = deadlock_vpk::Vpk::open(&args[0]).unwrap();
    std::fs::create_dir_all(&args[2]).unwrap();
    let mut count = 0;
    for path in vpk.entries.keys().filter(|p| p.contains(args[1].as_str())) {
        let bytes = vpk.read(path).unwrap();
        std::fs::write(std::path::Path::new(&args[2]).join(path.replace('/', "__")), bytes).unwrap();
        count += 1;
    }
    println!("extracted {count}");
}
