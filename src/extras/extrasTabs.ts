/** The Extras tab open per project id, for the session only (never saved): leaving the page
 * and coming back opens the same tab. */
export const openTabs = new Map<string, string>();
/** Tests start on the first tab. */
export function forgetExtrasTabs() {
  openTabs.clear();
}
