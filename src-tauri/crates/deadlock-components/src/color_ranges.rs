//! Hero color ranges: which base-color pixels a range selects, and how it
//! changes them. The function lives in `deadlock_project::color_ranges`, so the
//! replacement model's color editor (`deadlock-materials color_adjust`) uses the
//! same one; it is re-exported here for hero colors. The same function is
//! `src/extras/heroColorRanges.ts` (the Extras preview); both tests check
//! `tests/fixtures/hero-color-ranges.json`.
pub use deadlock_project::color_ranges::*;
