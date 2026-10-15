//! The reviewed TangTang data behind the `tangtang_r2` and `tangtang_reference`
//! presets: the 38 accepted groups and the two proxies. It is captured from a
//! third-party character and kept outside the public repository (private
//! assets, see `build.rs`), so it is compiled in only when the files were
//! present at build time. Without them every preset answers with
//! [`NOT_INCLUDED`] and Automatic authors the movement as for any other model.

/// Whether this build carries the TangTang presets.
pub const AVAILABLE: bool = cfg!(tangtang_assets);
/// The error for a TangTang preset in a build without the data.
pub const NOT_INCLUDED: &str =
    "The TangTang presets are not included in this build. Choose Automatic for the movement.";

/// `tangtang-r2-groups.json`: the accepted groups with their fit ranges.
#[cfg(tangtang_assets)]
pub const GROUPS: Option<&str> =
    Some(include_str!(concat!(env!("DMB_TANGTANG_ASSETS"), "/tangtang-r2-groups.json")));
#[cfg(not(tangtang_assets))]
pub const GROUPS: Option<&str> = None;

/// `tangtang-r2-proxy.dmx`: the accepted Haze conversion's proxy (`TANG_PROXY`).
#[cfg(tangtang_assets)]
pub const R2_PROXY: Option<&[u8]> =
    Some(include_bytes!(concat!(env!("DMB_TANGTANG_ASSETS"), "/tangtang-r2-proxy.dmx")));
#[cfg(not(tangtang_assets))]
pub const R2_PROXY: Option<&[u8]> = None;

/// `tangtang-hinged-streamers-proxy.dmx`: the reference hair, cloth and
/// hinged streamers (`TANG_REFERENCE_PROXY`).
#[cfg(tangtang_assets)]
pub const REFERENCE_PROXY: Option<&[u8]> = Some(include_bytes!(concat!(
    env!("DMB_TANGTANG_ASSETS"),
    "/tangtang-hinged-streamers-proxy.dmx"
)));
#[cfg(not(tangtang_assets))]
pub const REFERENCE_PROXY: Option<&[u8]> = None;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_data_is_all_there_or_all_absent() {
        assert_eq!(GROUPS.is_some(), AVAILABLE);
        assert_eq!(R2_PROXY.is_some(), AVAILABLE);
        assert_eq!(REFERENCE_PROXY.is_some(), AVAILABLE);
        if let Some(groups) = GROUPS {
            let saved: serde_json::Value = serde_json::from_str(groups).unwrap();
            assert_eq!(saved["groups"].as_array().unwrap().len(), 38);
        }
    }
}
