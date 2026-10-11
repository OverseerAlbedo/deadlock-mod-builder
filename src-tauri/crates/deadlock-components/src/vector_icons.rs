//! Read-only VSVG previews. Texture replacement contracts stay unchanged.
use crate::{Result, fail};
use deadlock_resource::container;

pub struct Preview {
    pub svg: Vec<u8>,
    pub width: u16,
    pub height: u16,
}

pub fn preview(raw: &[u8]) -> Result<Preview> {
    let layout = container::layout(raw).map_err(fail)?;
    let block = container::unique(&layout.blocks,"DATA").map_err(fail)?;
    let data = container::payload(raw,block).map_err(fail)?;
    if data.len()<6 || data.len()>4*1024*1024 { return Err(fail("Invalid vector icon size.")); }
    let crc = u32::from_le_bytes(data[..4].try_into().unwrap());
    let count = u16::from_le_bytes(data[4..6].try_into().unwrap());
    let mut offset = 6usize;
    for _ in 0..count {
        let end = data.get(offset..).and_then(|tail|tail.iter().position(|b|*b==0))
            .ok_or_else(||fail("Truncated vector icon image table."))?;
        offset = offset.checked_add(end+1+if layout.resource_version>=3 {8} else {4})
            .filter(|n|*n<=data.len()).ok_or_else(||fail("Invalid vector icon image table."))?;
    }
    let svg = &data[offset..];
    if crc32fast::hash(svg)!=crc { return Err(fail("Vector icon content checksum differs.")); }
    let text = std::str::from_utf8(svg).map_err(fail)?;
    let doc = roxmltree::Document::parse(text).map_err(fail)?;
    let root = doc.root_element();
    if root.tag_name().name()!="svg" { return Err(fail("Vector icon does not contain an SVG image.")); }
    // Image previews cannot load files, remote images, scripts or foreign HTML.
    // Internal paint/clip references are retained for normal game artwork.
    for node in doc.descendants().filter(|n|n.is_element()) {
        if ["script","foreignObject","image","animate","animateMotion","animateTransform","set","iframe","object","embed"]
            .contains(&node.tag_name().name()) { return Err(fail("This vector icon contains unsupported active or external content.")); }
        for attr in node.attributes() {
            let name = attr.name().to_ascii_lowercase();
            if name.starts_with("on") || (name=="href" && !attr.value().starts_with('#')) {
                return Err(fail("This vector icon contains unsupported active or external content."));
            }
        }
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("@import") || lower.split("url(").skip(1).any(|v|!v.trim_start().trim_start_matches(['\'','"']).starts_with('#')) {
        return Err(fail("External vector icon references cannot be previewed."));
    }
    let number = |value:Option<&str>| -> Option<u16> {
        value?.trim_end_matches("px").parse::<f64>().ok()
            .filter(|v|v.is_finite() && *v>0. && *v<=65535.).map(|v|v.ceil() as u16)
    };
    let view = root.attribute("viewBox").map(|s|s.split(|c:char|c.is_whitespace() || c==',').filter(|s|!s.is_empty()).collect::<Vec<_>>()).unwrap_or_default();
    let width = number(root.attribute("width")).or_else(||number(view.get(2).copied())).unwrap_or(0);
    let height = number(root.attribute("height")).or_else(||number(view.get(3).copied())).unwrap_or(0);
    Ok(Preview {svg:svg.to_vec(),width,height})
}
