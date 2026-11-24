/** Names of the Prepare model page's tabs and buttons that messages mention, in one place so a
 * rename is one edit here. The tab bar and the buttons use these labels; the Rust copies in
 * `deadlock-preparation/src/page_names.rs` must stay equal (its `tests/page_names.rs` reads this file). */
export const TAB_NAMES={roles:'Body roles',physics:'Moving parts',mapping:'Joint matches',weapon:'Gun',review:'Fit & review'} as const;
export const BUTTON_NAMES={
  /** Joint matches tab: suggestions again from the current body roles. */
  refreshMatches:'Refresh suggestions using my roles',
  /** Moving parts tab: keep only the saved groups that are still detected. */
  useDetectedGroups:'Use the detected groups',
  /** Model file & orientation (open before a check, a top-bar menu after it): inspect the model file again. */
  loadAndCheck:'Load and check only',
  /** Check results: the one-click fix of stale body-following matches (the same refresh). */
  refreshTheMatches:'Refresh the matches',
  /** Prepared output: open the converted model in the 3D view. */
  showResult:'Show the result in the 3D view',
  /** The action bar before a preparation: check the choices, and prepare. */
  checkForProblems:'Check for problems',
  prepareModel:'Prepare model',
  /** Fit & review after a preparation: prepare again with the current choices. */
  prepareAgain:'Prepare again',
} as const;
