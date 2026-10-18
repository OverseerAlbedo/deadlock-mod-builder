//! DepotDownloader's Steam QR sign-in code, read back from its output so the
//! app can show it. DepotDownloader 3.4 prints the code as text after "Use the
//! Steam Mobile App to sign in with this QR code:" (QRCoder
//! `GetLineByLineGraphic(1)`: one line per module row, a dark module "██", a
//! light one two spaces, a quiet zone of four modules) and prints a new one
//! after "The QR code has changed:" when Steam renews it. The app runs tools
//! without a console, so the block characters arrive in the console's code page
//! (byte 0xDB in code page 437, two bytes in the East Asian ones) and read back
//! as replacement characters. Only the layout is used: spaces are light,
//! anything else is dark, and the width of a dark module is measured on the
//! first row, which starts with a 7-module finder pattern.

/// The line DepotDownloader prints before each code.
pub const PROMPT: &str = "sign in with this QR code:";

/// What the output says about the QR sign-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignIn {
    /// No complete code yet (none printed, or one still arriving).
    Unknown,
    /// The newest code, waiting to be scanned: rows of modules (true = dark),
    /// without the quiet zone.
    Code(Vec<Vec<bool>>),
    /// DepotDownloader printed something else after the newest code: the
    /// sign-in finished or failed.
    Over,
}

/// Reads DepotDownloader's standard output so far.
pub fn read(output: &str) -> SignIn {
    let Some(at) = output.rfind(PROMPT) else { return SignIn::Unknown };
    let mut lines = output[at + PROMPT.len()..].split('\n').map(|l| l.trim_end_matches('\r'));
    // The rest of the prompt line.
    lines.next();
    let Some(code) = code(&mut lines) else { return SignIn::Unknown };
    // The quiet rows below the code and a renewal notice keep it current.
    if lines.any(|l| !l.trim().is_empty() && !l.contains("QR")) {
        SignIn::Over
    } else {
        SignIn::Code(code)
    }
}

/// The code at the start of `lines`, if it is complete and well formed.
fn code<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Option<Vec<Vec<bool>>> {
    let first = lines.find(|l| !l.trim().is_empty())?;
    let quiet_chars = first.len() - first.trim_start_matches(' ').len();
    let finder = first[quiet_chars..].chars().take_while(|c| *c != ' ').count();
    if quiet_chars % 2 != 0 || finder == 0 || finder % 7 != 0 {
        return None;
    }
    let width = finder / 7;
    let quiet = quiet_chars / 2;
    let top = modules(first, width)?;
    let size = top.iter().rposition(|dark| *dark)? + 1 - quiet;
    if !(21..=177).contains(&size) || (size - 17) % 4 != 0 {
        return None;
    }
    let row = |mut all: Vec<bool>| {
        all.resize(all.len().max(quiet + size), false);
        let inside = all[quiet..quiet + size].to_vec();
        (!all[..quiet].iter().chain(&all[quiet + size..]).any(|dark| *dark)).then_some(inside)
    };
    let mut rows = vec![row(top)?];
    for _ in 1..size {
        rows.push(row(modules(lines.next()?, width)?)?);
    }
    // The last row must be finished (followed by a line break).
    lines.next()?;
    finders(&rows).then_some(rows)
}

/// The modules of one printed row: two spaces are light, `width` other
/// characters are dark.
fn modules(line: &str, width: usize) -> Option<Vec<bool>> {
    let chars: Vec<char> = line.chars().collect();
    let mut modules = Vec::with_capacity(chars.len() / 2);
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            if chars.get(i + 1) != Some(&' ') {
                return None;
            }
            modules.push(false);
            i += 2;
        } else {
            if chars.get(i..i + width)?.contains(&' ') {
                return None;
            }
            modules.push(true);
            i += width;
        }
    }
    Some(modules)
}

/// The three finder patterns are in place.
fn finders(rows: &[Vec<bool>]) -> bool {
    let size = rows.len();
    [(0, 0), (0, size - 7), (size - 7, 0)].iter().all(|&(top, left)| {
        (0..7).all(|r| {
            (0..7).all(|c| {
                let ring = r == 0 || r == 6 || c == 0 || c == 6;
                let centre = (2..=4).contains(&r) && (2..=4).contains(&c);
                rows[top + r][left + c] == (ring || centre)
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 21×21 code: the three finder patterns, the timing patterns and a few
    /// data modules.
    fn sample() -> Vec<Vec<bool>> {
        let size = 21;
        let mut rows = vec![vec![false; size]; size];
        for (top, left) in [(0, 0), (0, size - 7), (size - 7, 0)] {
            for r in 0..7 {
                for c in 0..7 {
                    rows[top + r][left + c] = r == 0 || r == 6 || c == 0 || c == 6 || ((2..=4).contains(&r) && (2..=4).contains(&c));
                }
            }
        }
        for i in 8..size - 8 {
            rows[6][i] = i % 2 == 0;
            rows[i][6] = i % 2 == 0;
        }
        for (r, c) in [(9, 9), (10, 12), (15, 17), (20, 20), (12, 20)] {
            rows[r][c] = true;
        }
        rows
    }

    /// Prints `rows` as DepotDownloader does, with `dark` for a dark module.
    fn printed(rows: &[Vec<bool>], dark: &str) -> String {
        let blank = "  ".repeat(rows.len() + 8);
        let mut text = String::from("Logging in with QR code...\r\nUse the Steam Mobile App to sign in with this QR code:\r\n");
        for _ in 0..4 {
            text += &format!("{blank}\r\n");
        }
        for row in rows {
            let line: String = row.iter().map(|d| if *d { dark } else { "  " }).collect();
            text += &format!("        {line}        \r\n");
        }
        for _ in 0..4 {
            text += &format!("{blank}\r\n");
        }
        text
    }

    #[test]
    fn reads_the_code_in_any_console_code_page() {
        let rows = sample();
        // UTF-8, code page 437 read back as UTF-8 (one replacement character
        // per byte) and a double-byte code page (two per block character).
        for dark in ["██", "\u{FFFD}\u{FFFD}", "\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}", "\u{FFFD}i\u{FFFD}i"] {
            assert_eq!(read(&printed(&rows, dark)), SignIn::Code(rows.clone()), "{dark:?}");
        }
    }

    #[test]
    fn follows_renewed_codes_and_the_end_of_the_sign_in() {
        let rows = sample();
        let mut renewed = rows.clone();
        renewed[11][11] = true;
        let first = printed(&rows, "\u{FFFD}\u{FFFD}");
        let mut text = first.clone();
        // A renewal notice alone keeps the code current.
        text += "\r\nThe QR code has changed:\r\n";
        assert_eq!(read(&text), SignIn::Code(rows.clone()));
        text += &printed(&renewed, "\u{FFFD}\u{FFFD}")["Logging in with QR code...\r\n".len()..];
        assert_eq!(read(&text), SignIn::Code(renewed));
        // Anything else afterwards: signed in (or failed).
        assert_eq!(read(&(text + " Done!\r\n")), SignIn::Over);
        // Nothing printed yet, and a code still arriving (10 rows, then the
        // last row cut short).
        assert_eq!(read("Connecting to Steam3... Done!\r\nLogging in with QR code...\r\n"), SignIn::Unknown);
        let lines: Vec<&str> = first.split_inclusive('\n').collect();
        assert_eq!(read(&lines[..16].concat()), SignIn::Unknown);
        let last: String = lines[26].chars().take(20).collect();
        assert_eq!(read(&(lines[..26].concat() + &last)), SignIn::Unknown);
        assert_eq!(read(&lines[..27].concat()), SignIn::Code(rows.clone()));
    }

    #[test]
    fn rejects_damaged_codes() {
        let rows = sample();
        let text = printed(&rows, "\u{FFFD}\u{FFFD}");
        // One dark module printed one character short.
        let lines: Vec<&str> = text.split("\r\n").collect();
        let mut damaged = lines.clone();
        let short = lines[12].replacen("\u{FFFD}\u{FFFD}", "\u{FFFD}", 3);
        damaged[12] = &short;
        assert_eq!(read(&damaged.join("\r\n")), SignIn::Unknown);
        // A broken finder pattern.
        let mut wrong = rows.clone();
        wrong[3][3] = false;
        assert_eq!(read(&printed(&wrong, "\u{FFFD}\u{FFFD}")), SignIn::Unknown);
        // Not a code at all.
        assert_eq!(read(&format!("{PROMPT}\r\nhello world\r\n\r\n")), SignIn::Unknown);
    }
}
