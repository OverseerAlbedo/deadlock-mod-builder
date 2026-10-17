//! Development aid: the events of animation clips (.vnmclip_c), with their
//! timing and values, optionally only those whose text contains a filter.
//!
//!   clip_events FILTER CLIP.vnmclip_c [CLIP ...]
use deadlock_resource::{container, Kind, Value};
fn text(v: &Value, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    match &v.kind {
        Kind::Object(o) => o.iter().map(|(k, v)| match &v.kind {
            Kind::Object(_) | Kind::Array(_) => format!("{pad}{k}:\n{}", text(v, depth + 1)),
            _ => format!("{pad}{k} = {}\n", text(v, 0).trim()),
        }).collect(),
        Kind::Array(a) if a.iter().all(|x| !matches!(x.kind, Kind::Object(_) | Kind::Array(_))) && a.len() <= 16 =>
            format!("{pad}[{}]\n", a.iter().map(|x| text(x, 0).trim().to_string()).collect::<Vec<_>>().join(", ")),
        Kind::Array(a) => a.iter().enumerate().map(|(i, x)| format!("{pad}- [{i}]\n{}", text(x, depth + 1))).collect(),
        Kind::F32(n) => format!("{n}"), Kind::F64(n) => format!("{n}"),
        Kind::I64(n) => n.to_string(), Kind::U64(n) => n.to_string(),
        Kind::Bool(b) => b.to_string(), Kind::String(s) => format!("{s:?}"),
        other => format!("{other:?}").chars().take(60).collect(),
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for path in &args[1..] {
        let raw = std::fs::read(path).unwrap();
        let data = &container::read_blocks(&raw, "DATA").unwrap()[0].root;
        let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy().to_string();
        if std::env::var("KEYS").is_ok() {
            println!("{name}: {:?}", data.object().map(|o| o.iter().map(|(k, v)| format!("{k}{}", match &v.kind { Kind::Array(a) => format!("[{}]", a.len()), _ => String::new() })).collect::<Vec<_>>()));
        }
        let duration = data.get("m_flDuration").map(|v| text(v, 0)).unwrap_or_default();
        let frames = data.get("m_nNumFrames").map(|v| text(v, 0)).unwrap_or_default();
        for key in ["m_events", "m_syncTrack"] {
            let Some(events) = data.get(key) else { continue };
            let Ok(list) = events.array() else { continue };
            for event in list {
                let body = text(event, 1);
                if body.contains(args[0].as_str()) {
                    println!("{name} (duration {}, frames {}):\n{body}", duration.trim(), frames.trim());
                }
            }
        }
    }
}
