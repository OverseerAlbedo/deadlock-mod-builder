//! Keep the Windows user name out of logs people share. The user's profile
//! folder (C:\Users\<name>) is written as C:\Users\<you> in the session log, the
//! log shown in the app and exported logs. Other paths stay as they are.

/// The profile folder in the spellings logs use: backslashes, forward slashes
/// and JSON-escaped backslashes.
fn variants(profile: &str) -> Vec<(String, String)> {
    let profile = profile.trim_end_matches(['\\', '/']);
    let Some(split) = profile.rfind(['\\', '/']) else { return vec![] };
    if profile.len() <= split + 1 {
        return vec![];
    }
    let hidden = format!("{}<you>", &profile[..=split]);
    let mut out = vec![(profile.to_owned(), hidden.clone())];
    out.push((profile.replace('\\', "/"), hidden.replace('\\', "/")));
    out.push((profile.replace('\\', "\\\\"), hidden.replace('\\', "\\\\")));
    out.retain(|(from, _)| !from.is_empty());
    out.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Replace `profile` in `text`, ignoring ASCII case (Windows paths are not
/// case-sensitive).
pub fn scrub_with(text: &str, profile: &str) -> String {
    let mut result = text.to_owned();
    for (from, to) in variants(profile) {
        let needle = from.to_ascii_lowercase();
        let mut out = String::with_capacity(result.len());
        let lower = result.to_ascii_lowercase();
        let mut rest = 0;
        let mut from_here = 0;
        while let Some(found) = lower[from_here..].find(&needle) {
            let start = from_here + found;
            let end = start + needle.len();
            // Only the whole folder name (jayde, not jaydenmore).
            if lower.as_bytes().get(end).is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')) {
                from_here = start + 1;
                continue;
            }
            out.push_str(&result[rest..start]);
            out.push_str(&to);
            rest = end;
            from_here = end;
        }
        out.push_str(&result[rest..]);
        result = out;
    }
    result
}

/// `scrub_with` for the current user's profile folder (USERPROFILE).
pub fn scrub(text: &str) -> String {
    match std::env::var("USERPROFILE") {
        Ok(profile) if profile.len() > 3 => scrub_with(text, &profile),
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_profile_folder_is_hidden_in_every_spelling_and_case() {
        let profile = r"C:\Users\jayde";
        let text = r#"Full tool output: C:\Users\jayde\AppData\Roaming\x; \\?\c:\users\JAYDE\Documents; C:/Users/jayde/Desktop; {"path":"C:\\Users\\jayde\\a"}"#;
        let scrubbed = scrub_with(text, profile);
        assert!(!scrubbed.to_ascii_lowercase().contains("jayde"), "{scrubbed}");
        assert!(scrubbed.contains(r"C:\Users\<you>\AppData"));
        assert!(scrubbed.contains("C:/Users/<you>/Desktop"));
        assert!(scrubbed.contains(r#""C:\\Users\\<you>\\a""#));
        // Other users' folders and unrelated text stay.
        assert_eq!(scrub_with(r"C:\Users\jaydenmore\x and D:\Games", profile), r"C:\Users\jaydenmore\x and D:\Games");
    }
}
