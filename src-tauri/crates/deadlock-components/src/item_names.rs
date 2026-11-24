//! Extras → Items: renamed shop items. The shop's item names are loose text
//! files beside the game archive, one per language:
//! `resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_<language>.txt`.
//! A mod's file of that name is read INSTEAD of the game's (a file holding only
//! the changed names leaves every other item without a name), so the build
//! writes each language's WHOLE file: the game's own bytes with only the quoted
//! values of the changed tokens replaced.
//!
//! * The reader (`pairs`) keeps each value's byte span, so writing changes only
//!   those bytes (byte-order mark, line ends, comments and spacing untouched).
//!   `item_icons::Names` reads through the same function.
//! * For one renamed item in one language file (`plan`): its name token gets
//!   the new name; its `_search` token, when the file has one, keeps the file's
//!   words and gets the new name after a space (not when a run of its words
//!   already is the name, and never for a value holding an escape); with the
//!   `buff_names` switch, every other token whose value is exactly the item's
//!   name in that file (the buff names shown while the item works) gets it too,
//!   except other items' name tokens and every `_search` token.
//! * Every language file with tokens is written; stubs without tokens are not.
//!   A file the reader cannot carry exactly is skipped with a note, except the
//!   English file, which refuses the build.
//! * The install check reads the game's files again: a file whose digest
//!   changed since the build refuses the install (`GAME_NAMES_CHANGED`).
use crate::{Result, fail, sha};
use deadlock_core::{safety, text::casefold};
use deadlock_project::item_names::ItemNameRow;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

/// The names folder, under `resource/localization/` beside the archive and in a VPK.
pub const FOLDER: &str = "citadel_gc_mod_names";
/// Where the files go in a mod's VPK (with the trailing slash).
pub const PACKAGE_FOLDER: &str = "resource/localization/citadel_gc_mod_names/";
/// A language file is `<PREFIX><language>.txt`.
pub const PREFIX: &str = "citadel_gc_mod_names_";
pub const ENGLISH: &str = "english";
/// The largest names file read (the game's are about 40 KB).
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

pub const NAMES_MISSING: &str = "The game's item names were not found beside the game archive (resource/localization/citadel_gc_mod_names, with an English file that holds item names). Check the Deadlock folder in Settings.";
pub const UNKNOWN_ITEM: &str = "A renamed item is not in the game's item list now";
pub const NOT_CARRIED: &str = "The game's item names file could not be read and written back exactly";
pub const GAME_NAMES_CHANGED: &str = "The game's item list changed since this mod was built: build the mod again before installing.";
pub const ITEM_NAMES_CHANGED: &str = "Renamed items changed since this mod was built: build the mod again before installing.";

/// Item-name rules beyond the item's own name and search words, each behind
/// its own switch (each can be enabled or disabled for the app on its own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Switches {
    /// Rename the buff names (other tokens whose value is exactly the item's name).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub buff_names: bool,
}
impl Switches {
    pub fn is_off(&self) -> bool {
        *self == Self::default()
    }
}
/// The app's switches. `buff_names` is on: a renamed item's buff names (the
/// names the game shows for the item's effect while it works, held by other
/// tokens with exactly the item's name) take the new name too, never another
/// item's name token or any `_search` token.
pub const SWITCHES: Switches = Switches { buff_names: true };
#[cfg(feature = "object-test-switches")]
static TEST_SWITCHES: std::sync::Mutex<Option<Switches>> = std::sync::Mutex::new(None);
/// Other switches for an example or end-to-end test only, for example a build
/// made without a rule (feature `object-test-switches`, never enabled by the
/// app's own build).
#[cfg(feature = "object-test-switches")]
pub fn open_for_test(switches: Option<Switches>) {
    *TEST_SWITCHES.lock().unwrap_or_else(|e| e.into_inner()) = switches;
}
/// The switches in force: `SWITCHES` (or a test's own, see `open_for_test`).
pub fn switches() -> Switches {
    #[cfg(feature = "object-test-switches")]
    if let Some(test) = *TEST_SWITCHES.lock().unwrap_or_else(|e| e.into_inner()) {
        return test;
    }
    SWITCHES
}

/// One `"key" "value"` pair of a names file and where its value's bytes are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    /// The key as written, escapes resolved.
    pub key: String,
    /// The value's raw bytes (between the quotes, escapes kept), as offsets into the text.
    pub value: Range<usize>,
    /// Inside the file's `"Tokens" { … }` block (header keys such as `Language` are not).
    pub in_tokens: bool,
}
/// A quoted string's content with its escapes resolved (a backslash takes the next character).
pub fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}
/// Every `"key" "value"` pair of a names file, line by line: up to two quoted
/// strings per line (a backslash escapes the next character), nothing after
/// `//` outside a string, a leading byte-order mark skipped.
pub fn pairs(text: &str) -> Vec<Pair> {
    let start = text.len() - text.trim_start_matches('\u{feff}').len();
    let mut out = vec![];
    let mut depth = 0usize;
    let mut tokens_after_brace = false;
    let mut tokens_depth: Option<usize> = None;
    let mut offset = start;
    for line in text[start..].split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let lead = body.len() - body.trim_start().len();
        let trimmed = body.trim();
        let base = offset + lead;
        offset += line.len();
        // The quoted strings (the old reader's exact rules).
        let mut parts: Vec<(String, Range<usize>)> = vec![];
        let mut rest = trimmed;
        let mut at = 0usize;
        while parts.len() < 2 {
            let Some(open) = rest.find('"') else { break };
            if rest[..open].contains("//") {
                break;
            }
            let tail = &rest[open + 1..];
            let mut end = None;
            let mut chars = tail.char_indices();
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => {
                        end = Some(i);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(end) = end else { break };
            let from = at + open + 1;
            parts.push((unescape(&tail[..end]), base + from..base + from + end));
            at = from + end + 1;
            rest = &tail[end + 1..];
        }
        let depth_here = depth;
        if let [(key, _), (_, value)] = parts.as_slice() {
            out.push(Pair { key: key.clone(), value: value.clone(), in_tokens: tokens_depth.is_some_and(|d| depth_here >= d) });
        }
        if let [(key, _)] = parts.as_slice() {
            tokens_after_brace = casefold(key) == "tokens";
        }
        // Braces outside strings and comments move the depth.
        let mut quoted = false;
        let mut chars = trimmed.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            match c {
                '\\' if quoted => {
                    chars.next();
                }
                '"' => quoted = !quoted,
                '/' if !quoted && trimmed[i + 1..].starts_with('/') => break,
                '{' if !quoted => {
                    depth += 1;
                    if tokens_after_brace {
                        tokens_depth = Some(depth);
                        tokens_after_brace = false;
                    }
                }
                '}' if !quoted => {
                    if tokens_depth.is_some_and(|d| depth <= d) {
                        tokens_depth = None;
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }
    }
    out
}

/// Whether a token (casefolded) is an item's name token: `upgrade_…`, not a
/// `_search` token and not a `…/modifier_…` buff path.
pub fn is_item_token(key: &str) -> bool {
    key.starts_with("upgrade_") && !key.contains('/') && !key.ends_with("_search")
}

/// A names file read so that it can be written back with only some values changed.
#[derive(Debug, Clone)]
pub struct NamesFile {
    text: String,
    pairs: Vec<Pair>,
    /// Casefolded token key → its pair (tokens block only).
    tokens: BTreeMap<String, usize>,
}
impl NamesFile {
    /// Reads a file, or says why it cannot be carried exactly.
    pub fn read(raw: &[u8]) -> std::result::Result<Self, String> {
        let text = std::str::from_utf8(raw).map_err(|_| "it is not UTF-8 text".to_owned())?.to_owned();
        let pairs = pairs(&text);
        let mut tokens = BTreeMap::new();
        for (i, pair) in pairs.iter().enumerate() {
            if pair.in_tokens && tokens.insert(casefold(&pair.key), i).is_some() {
                return Err(format!("it holds the token {} twice", pair.key));
            }
        }
        Ok(Self { text, pairs, tokens })
    }
    pub fn has_tokens(&self) -> bool {
        !self.tokens.is_empty()
    }
    /// A token's raw value (escapes kept).
    pub fn raw(&self, key: &str) -> Option<&str> {
        self.tokens.get(&casefold(key)).map(|&i| &self.text[self.pairs[i].value.clone()])
    }
    /// The casefolded token keys, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.tokens.keys().map(String::as_str)
    }
    /// The file with these token values (casefolded key → new raw value) put in
    /// place; every other byte stays.
    pub fn write(&self, changes: &BTreeMap<String, String>) -> String {
        let mut spans: Vec<(Range<usize>, &str)> = changes
            .iter()
            .filter_map(|(key, value)| self.tokens.get(key).map(|&i| (self.pairs[i].value.clone(), value.as_str())))
            .collect();
        spans.sort_by_key(|(span, _)| span.start);
        let mut out = String::with_capacity(self.text.len() + 64 * spans.len());
        let mut at = 0;
        for (span, value) in spans {
            out.push_str(&self.text[at..span.start]);
            out.push_str(value);
            at = span.end;
        }
        out.push_str(&self.text[at..]);
        out
    }
    /// Whether `written` reads back as this file with exactly `changes`: the
    /// same tokens, the changed ones with their new values, all others as before.
    pub fn reads_back(&self, written: &str, changes: &BTreeMap<String, String>) -> bool {
        let Ok(back) = Self::read(written.as_bytes()) else { return false };
        back.tokens.len() == self.tokens.len()
            && self.keys().all(|key| back.raw(key) == changes.get(key).map(String::as_str).or_else(|| self.raw(key)))
    }
}

/// The changes one language file gets for the renamed items.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilePlan {
    /// Casefolded token key → new raw value.
    pub changes: BTreeMap<String, String>,
    /// Renamed items this file has no token for.
    pub missing: Vec<String>,
    pub notes: Vec<String>,
}
/// Whether a run of `words` (whitespace-separated) equals `name`'s words, ignoring case.
fn holds_name(words: &str, name: &str) -> bool {
    let words: Vec<String> = words.split_whitespace().map(casefold).collect();
    let name: Vec<String> = name.split_whitespace().map(casefold).collect();
    !name.is_empty() && words.windows(name.len()).any(|w| w == name.as_slice())
}
/// The changes of one language file (see the module notes), all taken from the
/// game's values, so the order of the rows does not matter.
pub fn plan(file: &NamesFile, rows: &[ItemNameRow], switches: Switches) -> FilePlan {
    let mut plan = FilePlan::default();
    // Buff tokens: claimed by which item, with which name.
    let mut buffs: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut contested = BTreeSet::new();
    for row in rows {
        let item = casefold(&row.item);
        let Some(own) = file.raw(&item) else {
            plan.missing.push(row.item.clone());
            continue;
        };
        plan.changes.insert(item.clone(), row.name.clone());
        let search = format!("{item}_search");
        if let Some(words) = file.raw(&search) {
            if words.contains('\\') {
                plan.notes.push(format!("The search words of {} hold an escape and were left as they are.", row.item));
            } else if !holds_name(words, &row.name) {
                let value = if words.trim().is_empty() { row.name.clone() } else { format!("{words} {}", row.name) };
                plan.changes.insert(search, value);
            }
        }
        if switches.buff_names && !own.trim().is_empty() {
            for key in file.keys() {
                if key == item || is_item_token(key) || key.ends_with("_search") || file.raw(key) != Some(own) {
                    continue;
                }
                match buffs.get(key) {
                    Some((_, name)) if *name != row.name => {
                        contested.insert(key.to_owned());
                    }
                    Some(_) => {}
                    None => {
                        buffs.insert(key.to_owned(), (row.item.clone(), row.name.clone()));
                    }
                }
            }
        }
    }
    for (key, (_, name)) in buffs {
        if contested.contains(&key) {
            plan.notes.push(format!("{key} holds the name of two renamed items with different new names and was left as it is."));
        } else {
            plan.changes.insert(key, name);
        }
    }
    plan
}

/// One language file a build writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageFile {
    pub language: String,
    /// Its path in the VPK (`resource/localization/citadel_gc_mod_names/…`).
    pub path: String,
    /// SHA-256 of the game's file it was made from.
    pub game_sha256: String,
    /// SHA-256 of the written file.
    pub sha256: String,
    /// How many tokens changed.
    pub changed: usize,
    /// Renamed items this language file has no token for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}
/// A language file left out because the reader cannot carry it exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skipped {
    pub file: String,
    pub reason: String,
}
/// What a build wrote for the renamed items (also the inspection's answer).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamesReceipt {
    /// The rows applied (enabled rows, in project order).
    pub rows: Vec<ItemNameRow>,
    /// The switches in force (absent while all are off).
    #[serde(default, skip_serializing_if = "Switches::is_off")]
    pub switches: Switches,
    pub files: Vec<LanguageFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<Skipped>,
    /// Rows whose item the game's English file does not name (inspection only:
    /// a build refuses them).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown: Vec<String>,
}
impl NamesReceipt {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.files.is_empty()
    }
    /// Whether these are the rows the receipt applied.
    pub fn same_rows(&self, rows: &[ItemNameRow]) -> bool {
        self.rows == rows
    }
    /// The packaged paths.
    pub fn packaged(&self) -> Vec<String> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }
    /// The notes Analyze shows.
    pub fn notes(&self) -> Vec<String> {
        let mut out: Vec<String> = self.unknown.iter().map(|item| unknown_item(item)).collect();
        for skipped in &self.skipped {
            out.push(format!("Item names: {} was not written ({}).", skipped.file, skipped.reason));
        }
        for file in &self.files {
            if !file.missing.is_empty() {
                out.push(format!("Item names: the {} file has no name for {}; it keeps the game's.", file.language, file.missing.join(", ")));
            }
            out.extend(file.notes.iter().map(|n| format!("Item names ({}): {n}", file.language)));
        }
        out
    }
}
fn unknown_item(item: &str) -> String {
    format!("{UNKNOWN_ITEM}: {item}. Remove its renamed item row in Extras → Items.")
}
/// Whether a packaged path (casefolded) is a names file of this kind.
pub fn is_names_file(path: &str) -> bool {
    let path = casefold(path);
    path.strip_prefix(PACKAGE_FOLDER).is_some_and(|name| name.starts_with(PREFIX) && name.ends_with(".txt") && !name.contains('/'))
}
/// The game's names folder beside an archive (`<folder of pak01_dir.vpk>/resource/localization/citadel_gc_mod_names`).
pub fn folder(archive: &Path) -> PathBuf {
    archive.parent().unwrap_or(Path::new("")).join("resource").join("localization").join(FOLDER)
}
/// The language files of a names folder (file name as the game spells it,
/// language), sorted by name; `None` when the folder cannot be listed.
pub fn language_files(folder: &Path) -> Option<Vec<(String, String)>> {
    let mut out = vec![];
    for entry in std::fs::read_dir(folder).ok()? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let key = casefold(&name);
        let Some(language) = key.strip_prefix(PREFIX).and_then(|rest| rest.strip_suffix(".txt")) else { continue };
        if !language.is_empty() && entry.path().is_file() {
            out.push((name.clone(), language.to_owned()));
        }
    }
    out.sort_by_key(|(name, _)| casefold(name));
    Some(out)
}
static FILE_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// How many names files this process has read (tests: a project without
/// renamed items never reads them).
pub fn file_reads() -> usize {
    FILE_READS.load(std::sync::atomic::Ordering::SeqCst)
}
fn read_names(path: &Path) -> Result<Vec<u8>> {
    FILE_READS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    deadlock_vpk::check_unlinked_path(path, false).map_err(fail)?;
    if std::fs::metadata(path).map_err(fail)?.len() > MAX_FILE_BYTES {
        return Err(fail(format!("{NOT_CARRIED}: {} is larger than 4 MiB.", path.display())));
    }
    std::fs::read(path).map_err(fail)
}
/// The game's English names file of a folder, read (`NAMES_MISSING` without one that holds tokens).
pub fn english(folder: &Path) -> Result<NamesFile> {
    let files = language_files(folder).ok_or_else(|| fail(NAMES_MISSING))?;
    let (name, _) = files.iter().find(|(_, language)| language == ENGLISH).ok_or_else(|| fail(NAMES_MISSING))?;
    let file = NamesFile::read(&read_names(&folder.join(name))?).map_err(|reason| fail(format!("{NOT_CARRIED}: {name} ({reason}).")))?;
    if !file.has_tokens() {
        return Err(fail(NAMES_MISSING));
    }
    Ok(file)
}

/// The files the rows give from a names folder, and what was written.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub receipt: NamesReceipt,
    /// Package path → bytes, for every language file with tokens that was carried.
    pub outputs: BTreeMap<String, Vec<u8>>,
}
/// The renamed items applied to every language file with tokens in `folder`.
/// Rows whose item the English file does not name are listed in
/// `receipt.unknown` and applied nowhere.
pub fn render(folder: &Path, rows: &[ItemNameRow], switches: Switches) -> Result<Rendered> {
    let files = language_files(folder).ok_or_else(|| fail(NAMES_MISSING))?;
    let english = english(folder)?;
    let mut receipt = NamesReceipt { rows: rows.to_vec(), switches, ..Default::default() };
    let known: Vec<ItemNameRow> = rows
        .iter()
        .filter(|row| {
            let ok = is_item_token(&casefold(&row.item)) && english.raw(&row.item).is_some();
            if !ok {
                receipt.unknown.push(row.item.clone());
            }
            ok
        })
        .cloned()
        .collect();
    let mut outputs = BTreeMap::new();
    for (name, language) in files {
        let raw = read_names(&folder.join(&name))?;
        let refuse = |reason: &str| fail(format!("{NOT_CARRIED}: {name} ({reason})."));
        let file = match NamesFile::read(&raw) {
            Ok(file) => file,
            Err(reason) if language == ENGLISH => return Err(refuse(&reason)),
            Err(reason) => {
                receipt.skipped.push(Skipped { file: name, reason });
                continue;
            }
        };
        if !file.has_tokens() {
            continue;
        }
        let plan = plan(&file, &known, switches);
        let written = file.write(&plan.changes);
        if !file.reads_back(&written, &plan.changes) {
            let reason = "it does not read back as the game's file with only the new names";
            if language == ENGLISH {
                return Err(refuse(reason));
            }
            receipt.skipped.push(Skipped { file: name, reason: reason.into() });
            continue;
        }
        let path = format!("{PACKAGE_FOLDER}{name}");
        receipt.files.push(LanguageFile {
            language,
            path: path.clone(),
            game_sha256: sha(&raw),
            sha256: sha(written.as_bytes()),
            changed: plan.changes.len(),
            missing: plan.missing,
            notes: plan.notes,
        });
        outputs.insert(path, written.into_bytes());
    }
    Ok(Rendered { receipt, outputs })
}
/// Analyze: what a build would write, with unknown items listed (not refused).
pub fn inspect(archive: &Path, rows: &[ItemNameRow]) -> Result<NamesReceipt> {
    Ok(render(&folder(archive), rows, switches())?.receipt)
}
/// Build: writes every language file into `game` (by its package path) and
/// returns the receipt; refuses a row whose item the game does not have.
pub fn stage(archive: &Path, rows: &[ItemNameRow], game: &Path) -> Result<NamesReceipt> {
    let rendered = render(&folder(archive), rows, switches())?;
    if let Some(item) = rendered.receipt.unknown.first() {
        return Err(fail(unknown_item(item)));
    }
    for (path, bytes) in &rendered.outputs {
        safety::atomic_write(&safety::inside(game, path)?, bytes.clone())?;
    }
    Ok(rendered.receipt)
}
/// Install and frozen-build check: the game's files are the ones the build
/// read, and every packaged file (in `directory`, by package path) is what the
/// rows give from them now. Free without rows.
pub fn verify_entries(archive: &Path, receipt: &NamesReceipt, directory: &Path) -> Result<()> {
    if receipt.is_empty() {
        return Ok(());
    }
    let folder = folder(archive);
    for file in &receipt.files {
        let name = file.path.strip_prefix(PACKAGE_FOLDER).unwrap_or(&file.path);
        let now = read_names(&folder.join(name)).map(|raw| sha(&raw)).ok();
        if now.as_deref() != Some(file.game_sha256.as_str()) {
            return Err(fail(GAME_NAMES_CHANGED));
        }
    }
    if receipt.switches != switches() || !receipt.unknown.is_empty() {
        return Err(fail(ITEM_NAMES_CHANGED));
    }
    let rendered = render(&folder, &receipt.rows, receipt.switches).map_err(|_| fail(GAME_NAMES_CHANGED))?;
    if !rendered.receipt.unknown.is_empty() || rendered.receipt.files.iter().map(|f| &f.path).ne(receipt.files.iter().map(|f| &f.path)) {
        return Err(fail(GAME_NAMES_CHANGED));
    }
    if rendered.receipt != *receipt {
        return Err(fail(ITEM_NAMES_CHANGED));
    }
    for (path, bytes) in &rendered.outputs {
        let packaged = std::fs::read(safety::inside(directory, path)?).map_err(|_| fail(ITEM_NAMES_CHANGED))?;
        if packaged != *bytes {
            return Err(fail(ITEM_NAMES_CHANGED));
        }
    }
    Ok(())
}

/// What the Items page shows about renamed items: the game's item name tokens
/// with their English names, and the languages a build would write.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameNames {
    /// Whether the English file with item names was found.
    pub found: bool,
    /// Casefolded item token → the game's English name (escapes resolved).
    pub names: BTreeMap<String, String>,
    /// The languages with tokens (a build writes one file each), sorted.
    pub languages: Vec<String>,
}
/// The game's item names beside an archive; `found: false` without them.
pub fn game_names(archive: &Path) -> GameNames {
    let folder = folder(archive);
    let Ok(english) = english(&folder) else { return GameNames::default() };
    let names = english.keys().filter(|k| is_item_token(k)).map(|k| (k.to_owned(), unescape(english.raw(k).unwrap_or_default()))).collect();
    let languages = language_files(&folder)
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, _)| read_names(&folder.join(name)).ok().and_then(|raw| NamesFile::read(&raw).ok()).is_some_and(|f| f.has_tokens()))
        .map(|(_, language)| language)
        .collect();
    GameNames { found: true, names, languages }
}
/// One saved row as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowState {
    pub item: String,
    /// The game's English name of the item now (absent when the game has no such item).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_name: Option<String>,
    /// Whether the row applies: the game has the item (a switched-off row applies when switched on).
    pub known: bool,
}
/// Each saved row against the game's names, in row order.
pub fn row_states(game: &GameNames, rows: &[ItemNameRow]) -> Vec<RowState> {
    rows.iter()
        .map(|row| {
            let game_name = game.names.get(&casefold(&row.item)).cloned();
            RowState { item: row.item.clone(), known: game_name.is_some(), game_name }
        })
        .collect()
}
/// The stamp of a names folder (each language file's name, size and modified
/// time), so a kept answer is reused only while the game's files are the same.
pub fn folder_stamp(archive: &Path) -> Vec<(String, u64, u128)> {
    let folder = folder(archive);
    language_files(&folder)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _)| {
            let meta = std::fs::metadata(folder.join(&name)).ok();
            let modified = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
            (name, meta.map(|m| m.len()).unwrap_or(0), modified)
        })
        .collect()
}

/// Installed mods: how many of the game's current tokens a mod's names file
/// lacks (`None` when either file cannot be read as a names file).
pub fn missing_from(mod_file: &[u8], game_file: &[u8]) -> Option<usize> {
    let (ours, game) = (NamesFile::read(mod_file).ok()?, NamesFile::read(game_file).ok()?);
    Some(game.keys().filter(|key| ours.raw(key).is_none()).count())
}
/// Installed mods: the row's note for names files that lack some of the game's
/// tokens (language, count), or `None` when none lacks any.
pub fn outdated_note(missing: &[(String, usize)]) -> Option<String> {
    let lacking: Vec<&(String, usize)> = missing.iter().filter(|(_, n)| *n > 0).collect();
    let most = lacking.iter().map(|(_, n)| *n).max()?;
    let same = lacking.iter().all(|(_, n)| *n == most);
    let count = if same { most.to_string() } else { format!("Up to {most}") };
    let place = match lacking.as_slice() {
        [(language, _)] => format!("this file ({language})"),
        many => format!("its item names files ({} languages)", many.len()),
    };
    let verb = if count == "1" { "is" } else { "are" };
    Some(format!("{count} of the game's item names {verb} missing from {place}: those items show without a name until the mod is built again."))
}
