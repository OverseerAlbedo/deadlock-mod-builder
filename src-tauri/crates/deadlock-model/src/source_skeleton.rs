//! Source identity view only: preserve authored node spellings and bytes.
use super::{invalid, BoneRef, Result};
pub use deadlock_skeleton::source_skeleton::SourceAuditReport;
use std::collections::BTreeMap;

pub fn canonical_source_hierarchy<'a>(
    bones: BTreeMap<String, BoneRef<'a>>,
    report: Option<&SourceAuditReport>,
) -> Result<BTreeMap<String, BoneRef<'a>>> {
    let aliases = match report {
        Some(report) => deadlock_skeleton::source_skeleton::canonical_aliases(report)
            .map_err(|e| invalid(e.to_string()))?,
        None => BTreeMap::new(),
    };
    let mut resolved = BTreeMap::new();
    for (name, mut item) in bones {
        let canonical = aliases.get(&name).cloned().unwrap_or(name);
        item.parent = item
            .parent
            .map(|name| aliases.get(&name).cloned().unwrap_or(name));
        if resolved.insert(canonical.clone(), item).is_some() {
            return Err(invalid(format!(
                "Ambiguous source skeleton export aliases for bone: {canonical}"
            )));
        }
    }
    Ok(resolved)
}
