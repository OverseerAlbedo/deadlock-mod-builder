//! Developer differential-test harness; not part of the desktop runtime.
fn main() -> deadlock_core::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let source = std::path::PathBuf::from(args.next().expect("source"));
    let output = std::path::PathBuf::from(args.next().expect("packet output"));
    let packet = deadlock_preview::gltf::load_packet(
        &source,
        &deadlock_core::CancellationToken::default(),
        &deadlock_core::TimingRecorder::new("fixture"),
    )?;
    std::fs::write(output, packet)
        .map_err(|e| deadlock_core::CoreError::io("Write fixture packet", e))
}
