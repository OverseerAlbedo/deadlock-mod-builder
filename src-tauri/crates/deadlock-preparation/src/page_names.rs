//! Names of the Prepare model page's tabs and buttons that preparation messages
//! mention, so a message names what is on the page. They must equal the labels in
//! `src/preparation/pageNames.ts` (checked by `tests/page_names.rs`); rename a tab
//! or button in both places. The help steps in `src-tauri/src/troubleshooting.rs`
//! (rule on these messages) spell them out too; its test reads these constants.
pub const MOVING_PARTS_TAB: &str = "Moving parts";
pub const JOINT_MATCHES_TAB: &str = "Joint matches";
pub const GUN_TAB: &str = "Gun";
/// Joint matches tab: suggestions again from the current body roles.
pub const REFRESH_MATCHES: &str = "Refresh suggestions using my roles";
/// Check results: the one-click fix of stale body-following matches (the same refresh).
pub const REFRESH_THE_MATCHES: &str = "Refresh the matches";
/// Moving parts tab and check results: keep only the saved groups that are still detected.
pub const USE_DETECTED_GROUPS: &str = "Use the detected groups";
/// Model file & orientation (open before a check, a top-bar menu after it): inspect the model file again.
pub const LOAD_AND_CHECK: &str = "Load and check only";
