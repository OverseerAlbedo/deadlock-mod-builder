//! Renamed shop items: the names file reader and writer, the per-file rules,
//! every language of a folder, and the install check, on small made-up names
//! files in the game's format. The proof against the real game is ignored.
use deadlock_components::item_icons::Names;
use deadlock_components::item_names::{self, NamesFile, Switches};
use deadlock_project::item_names::ItemNameRow;
use std::collections::BTreeMap;
use std::path::Path;

/// The app's switches (the buff names rule on).
const APP: Switches = item_names::SWITCHES;
/// The rules without the buff names rule.
const WITHOUT_BUFFS: Switches = Switches { buff_names: false };

#[test]
fn the_app_s_switches_are_the_enabled_rules() {
    assert_eq!(item_names::SWITCHES, Switches { buff_names: true });
    assert_eq!(item_names::switches(), item_names::SWITCHES, "no test switches in this crate's own tests");
    assert!(!item_names::SWITCHES.is_off());
}

/// A made-up English file: a byte-order mark, CRLF, tabs, comments, an item with
/// and one without a search token, a buff token and a buff path holding an
/// item's name, two items sharing a name, and a search token of one holding
/// the other's name.
fn english() -> String {
    [
        "\u{feff}\"lang\"",
        "{",
        "\t\"Language\"\t\t\"English\"",
        "\t\"Language_search\"\t\t\"English\"",
        "\t\"Tokens\"",
        "\t{",
        "\t\t//------------------------",
        "\t\t// Upgrades: Sample",
        "\t\t//------------------------",
        "\t\t\"upgrade_sample_rite\"\t\t\t\t\t\"Sample Rite\"",
        "\t\t\"upgrade_sample_rite_search\"\t\t\t\"Sample Rite SR\"",
        "\t\t\"upgrade_sample_rite/modifier_sample_regen\"\t\"Sample Rite\"",
        "\t\t\"modifier_sample_regen\"\t\t\t\t\"Sample Rite\"",
        "\t\t\"upgrade_plain_item\"\t\t\t\t\"Plain Item\"",
        "\t\t// \"upgrade_commented\"\t\"Not Read\"",
        "\t\t\"upgrade_twin_a\"\t\t\"Twin\"",
        "\t\t\"upgrade_twin_a_search\"\t\"Twin\"",
        "\t\t\"upgrade_twin_b\"\t\t\"Twin\"",
        "\t\t\"upgrade_twin_b_search\"\t\"Twin TB\"",
        "\t\t\"modifier_twin\"\t\t\"Twin\"",
        "",
        "\t}",
        "}",
        "",
    ]
    .join("\r\n")
}
/// A made-up second language: the same keys in other words, no search token
/// for one item, and a search value holding an escape.
fn german() -> String {
    english()
        .replace("\"English\"", "\"German\"")
        .replace("\"Sample Rite\"", "\"Probe Ritus\"")
        .replace("\"Sample Rite SR\"", "\"Probe Ritus PR\"")
        .replace("\"Plain Item\"", "\"Schlichtes Ding\"")
        .replace("\t\t\"upgrade_twin_b_search\"\t\"Twin TB\"\r\n", "")
        .replace("\"upgrade_twin_a_search\"\t\"Twin\"", "\"upgrade_twin_a_search\"\t\"Zwilling \\\"Z\\\"\"")
}
/// A stub: a tokens block with no tokens.
const STUB: &str = "\u{feff}\"lang\"\r\n{\r\n\t\"Language\"\t\t\"danish\"\r\n\t\"Tokens\"\r\n\t{\r\n\t}\r\n}\r\n";

fn row(item: &str, name: &str) -> ItemNameRow {
    ItemNameRow { item: item.into(), enabled: true, name: name.into() }
}
fn read(text: &str) -> NamesFile {
    NamesFile::read(text.as_bytes()).unwrap()
}
/// The lines of `a` and `b` that differ, as (game, ours).
fn differing(a: &str, b: &str) -> Vec<(String, String)> {
    let (x, y): (Vec<_>, Vec<_>) = (a.split_inclusive('\n').collect(), b.split_inclusive('\n').collect());
    assert_eq!(x.len(), y.len(), "the same number of lines");
    x.iter().zip(&y).filter(|(p, q)| p != q).map(|(p, q)| (p.to_string(), q.to_string())).collect()
}

/// The reader as it was before the names writer (one copy kept here to prove
/// `Names::parse` reads every file exactly as before).
fn old_names(text: &str) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let mut parts = vec![];
        let mut rest = line.trim();
        while parts.len() < 2 {
            let Some(start) = rest.find('"') else { break };
            if rest[..start].contains("//") {
                break;
            }
            let tail = &rest[start + 1..];
            let mut value = String::new();
            let mut end = None;
            let mut chars = tail.char_indices();
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => {
                        if let Some((_, next)) = chars.next() {
                            value.push(next);
                        }
                    }
                    '"' => {
                        end = Some(i);
                        break;
                    }
                    c => value.push(c),
                }
            }
            let Some(end) = end else { break };
            parts.push(value);
            rest = &tail[end + 1..];
        }
        if let [key, value] = parts.as_slice() {
            names.entry(deadlock_core::text::casefold(key)).or_insert_with(|| value.clone());
        }
    }
    names
}
fn new_names(text: &str) -> BTreeMap<String, String> {
    let names = Names::parse(text);
    let keys: Vec<String> = names.keys().map(str::to_owned).collect();
    // `get` hides empty and `#` values; compare through the raw pairs instead.
    let mut out = BTreeMap::new();
    for pair in item_names::pairs(text) {
        out.entry(deadlock_core::text::casefold(&pair.key)).or_insert_with(|| item_names::unescape(&text[pair.value]));
    }
    assert_eq!(keys, out.keys().cloned().collect::<Vec<_>>());
    out
}

#[test]
fn the_item_icon_reader_reads_exactly_as_before() {
    for text in [
        english(),
        german(),
        STUB.to_owned(),
        "\u{feff}\u{feff}\"a\" \"b\" \"c\"\n  \"x\\\\\" \"y // z\" // \"q\" \"r\"\n\"open \"never\n\"k\"\t\"v\\\"w\"\r\n\"k\" \"second\"\n\"e\" \"\"\n\"t\" \"\\".into(),
        "no quotes at all\n// \"c\" \"d\"\n  \"k2\" // \"v2\"\n".into(),
    ] {
        assert_eq!(new_names(&text), old_names(&text), "{text:?}");
    }
}

#[test]
fn reading_keeps_every_byte_and_marks_the_tokens_block() {
    let text = english();
    let file = read(&text);
    assert_eq!(file.write(&BTreeMap::new()), text, "unchanged: the same bytes");
    assert_eq!(file.raw("UPGRADE_SAMPLE_RITE"), Some("Sample Rite"));
    assert_eq!(file.raw("language"), None, "header keys are not tokens");
    assert_eq!(file.raw("upgrade_commented"), None, "a commented-out line is not read");
    assert!(file.keys().all(|k| k == k.to_lowercase()));
    let pairs = item_names::pairs(&text);
    assert!(pairs.iter().any(|p| p.key == "Language" && !p.in_tokens));
    assert!(pairs.iter().filter(|p| p.in_tokens).count() == file.keys().count());
    assert!(!read(STUB).has_tokens());
    // What cannot be carried: not UTF-8, a token twice.
    assert!(NamesFile::read(b"\xff\xfe\"a\"").unwrap_err().contains("UTF-8"));
    let twice = text.replace("\"modifier_twin\"", "\"Upgrade_Plain_Item\"");
    assert!(NamesFile::read(twice.as_bytes()).unwrap_err().contains("twice"));
}

#[test]
fn a_rename_changes_only_the_value_bytes_of_its_name_search_and_buff_tokens() {
    let text = english();
    let file = read(&text);
    let plan = item_names::plan(&file, &[row("upgrade_sample_rite", "New Rite")], APP);
    let written = file.write(&plan.changes);
    assert!(file.reads_back(&written, &plan.changes));
    let name = ("\t\t\"upgrade_sample_rite\"\t\t\t\t\t\"Sample Rite\"\r\n".to_owned(), "\t\t\"upgrade_sample_rite\"\t\t\t\t\t\"New Rite\"\r\n".to_owned());
    let search = ("\t\t\"upgrade_sample_rite_search\"\t\t\t\"Sample Rite SR\"\r\n".to_owned(), "\t\t\"upgrade_sample_rite_search\"\t\t\t\"Sample Rite SR New Rite\"\r\n".to_owned());
    assert_eq!(
        differing(&text, &written),
        [
            name.clone(),
            search.clone(),
            ("\t\t\"upgrade_sample_rite/modifier_sample_regen\"\t\"Sample Rite\"\r\n".into(), "\t\t\"upgrade_sample_rite/modifier_sample_regen\"\t\"New Rite\"\r\n".into()),
            ("\t\t\"modifier_sample_regen\"\t\t\t\t\"Sample Rite\"\r\n".into(), "\t\t\"modifier_sample_regen\"\t\t\t\t\"New Rite\"\r\n".into()),
        ],
        "with the app's switches the buff tokens take the new name"
    );
    assert!(written.starts_with('\u{feff}') && written.ends_with("}\r\n"));
    // Without the buff names rule the buff tokens keep the game's name.
    let without = item_names::plan(&file, &[row("upgrade_sample_rite", "New Rite")], WITHOUT_BUFFS);
    assert_eq!(differing(&text, &file.write(&without.changes)), [name, search]);
    // The search words are extended once: a run of them already the name adds nothing.
    let again = item_names::plan(&file, &[row("upgrade_sample_rite", "sample  RITE")], APP);
    assert_eq!(again.changes.len(), 3, "{:?}", again.changes);
    assert_eq!(again.changes["upgrade_sample_rite"], "sample  RITE");
    assert!(!again.changes.contains_key("upgrade_sample_rite_search"));
    let partial = item_names::plan(&file, &[row("upgrade_sample_rite", "Rite SR")], APP);
    assert!(!partial.changes.contains_key("upgrade_sample_rite_search"));
    let not_a_run = item_names::plan(&file, &[row("upgrade_sample_rite", "Sample SR")], APP);
    assert_eq!(not_a_run.changes["upgrade_sample_rite_search"], "Sample Rite SR Sample SR");
}

#[test]
fn an_item_without_a_search_token_gets_only_its_name() {
    let text = english();
    let file = read(&text);
    let plan = item_names::plan(&file, &[row("upgrade_plain_item", "Plainer")], APP);
    assert_eq!(plan.changes, BTreeMap::from([("upgrade_plain_item".to_owned(), "Plainer".to_owned())]), "no search token is invented");
    assert_eq!(differing(&text, &file.write(&plan.changes)).len(), 1);
}

#[test]
fn buff_names_follow_the_item_and_never_another_item_or_a_search_token() {
    let file = read(&english());
    let plan = item_names::plan(&file, &[row("upgrade_sample_rite", "New Rite")], APP);
    assert_eq!(plan.changes["upgrade_sample_rite/modifier_sample_regen"], "New Rite");
    assert_eq!(plan.changes["modifier_sample_regen"], "New Rite");
    assert_eq!(plan.changes.len(), 4);
    // Without the rule only the name and its search words change.
    let without = item_names::plan(&file, &[row("upgrade_sample_rite", "New Rite")], WITHOUT_BUFFS);
    assert_eq!(without.changes.keys().collect::<Vec<_>>(), ["upgrade_sample_rite", "upgrade_sample_rite_search"]);
    // Twins: the other item's name token keeps "Twin", and this twin's search
    // token (its value exactly "Twin") is only extended; the buff follows.
    let twins = item_names::plan(&file, &[row("upgrade_twin_a", "Solo")], APP);
    assert_eq!(
        twins.changes,
        BTreeMap::from([
            ("upgrade_twin_a".to_owned(), "Solo".to_owned()),
            ("upgrade_twin_a_search".to_owned(), "Twin Solo".to_owned()),
            ("modifier_twin".to_owned(), "Solo".to_owned()),
        ])
    );
    // Both twins renamed differently: the shared buff is left as it is, with a note.
    let both = item_names::plan(&file, &[row("upgrade_twin_a", "Solo"), row("upgrade_twin_b", "Duo")], APP);
    assert!(!both.changes.contains_key("modifier_twin"));
    assert!(both.notes.iter().any(|n| n.contains("modifier_twin")));
    let same = item_names::plan(&file, &[row("upgrade_twin_a", "Pair"), row("upgrade_twin_b", "Pair")], APP);
    assert_eq!(same.changes["modifier_twin"], "Pair");
    // Row order never matters: every change comes from the game's values.
    let reversed = item_names::plan(&file, &[row("upgrade_twin_b", "Duo"), row("upgrade_twin_a", "Solo")], APP);
    assert_eq!(reversed.changes, both.changes);
}

#[test]
fn a_second_language_keeps_its_own_words_and_escapes() {
    let text = german();
    let file = read(&text);
    let plan = item_names::plan(&file, &[row("upgrade_sample_rite", "New Rite"), row("upgrade_twin_a", "Solo"), row("upgrade_twin_b", "Duo")], APP);
    assert_eq!(plan.changes["upgrade_sample_rite_search"], "Probe Ritus PR New Rite");
    assert_eq!(plan.changes["modifier_sample_regen"], "New Rite", "this language's buff names follow too");
    assert!(!plan.changes.contains_key("upgrade_twin_a_search"), "a value with an escape is left as it is");
    assert!(plan.notes.iter().any(|n| n.contains("upgrade_twin_a")));
    assert!(!plan.changes.contains_key("upgrade_twin_b_search"), "this language has no search token for it");
    let written = file.write(&plan.changes);
    assert!(file.reads_back(&written, &plan.changes));
    assert!(written.contains("\"Zwilling \\\"Z\\\"\""), "the escaped value is kept byte for byte");
    let lacking = read(&text.replace("\t\t\"upgrade_plain_item\"\t\t\t\t\"Schlichtes Ding\"\r\n", ""));
    let missing = item_names::plan(&lacking, &[row("upgrade_plain_item", "Plainer")], APP);
    assert!(missing.changes.is_empty());
    assert_eq!(missing.missing, ["upgrade_plain_item"]);
}

/// A made-up game folder: `<root>/pak01_dir.vpk` (never opened) and the names
/// folder beside it with English, German and a stub.
fn game(root: &Path, files: &[(&str, &[u8])]) -> std::path::PathBuf {
    let folder = item_names::folder(&root.join("pak01_dir.vpk"));
    std::fs::create_dir_all(&folder).unwrap();
    for (name, bytes) in files {
        std::fs::write(folder.join(name), bytes).unwrap();
    }
    root.join("pak01_dir.vpk")
}
fn standard(root: &Path) -> std::path::PathBuf {
    game(
        root,
        &[
            ("citadel_gc_mod_names_english.txt", english().as_bytes()),
            ("citadel_gc_mod_names_german.txt", german().as_bytes()),
            ("citadel_gc_mod_names_danish.txt", STUB.as_bytes()),
            ("readme.txt", b"not a names file"),
        ],
    )
}

#[test]
fn every_language_with_tokens_is_written_and_stubs_are_not() {
    let temp = tempfile::tempdir().unwrap();
    let archive = standard(temp.path());
    let out = temp.path().join("game-out");
    let receipt = item_names::stage(&archive, &[row("upgrade_sample_rite", "New Rite")], &out).unwrap();
    let paths: Vec<_> = receipt.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_english.txt", "resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_german.txt"]);
    assert!(receipt.files.iter().all(|f| f.changed == 4 && item_names::is_names_file(&f.path)), "name, search words and two buff names");
    assert_eq!(receipt.files[0].game_sha256, deadlock_components::resource_fingerprint(english().as_bytes()));
    let written = std::fs::read_to_string(out.join(&receipt.files[1].path)).unwrap();
    assert_eq!(deadlock_components::resource_fingerprint(written.as_bytes()), receipt.files[1].sha256);
    assert!(written.contains("\"New Rite\"") && written.contains("\"Probe Ritus PR New Rite\""));
    assert!(!written.contains("\"Probe Ritus\""), "the buff names follow in every language");
    assert!(!out.join("resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_danish.txt").exists());
    assert!(receipt.skipped.is_empty() && receipt.unknown.is_empty());
    assert_eq!(receipt.switches, item_names::SWITCHES, "built with the app's switches");
    // The receipt keeps its keys short: no skips or unknown items when there are none.
    let json = serde_json::to_value(&receipt).unwrap();
    assert_eq!(json.as_object().unwrap().keys().collect::<Vec<_>>(), ["rows", "switches", "files"]);
    assert_eq!(json["switches"], serde_json::json!({"buff_names": true}));
    item_names::verify_entries(&archive, &receipt, &out).unwrap();
}

#[test]
fn a_language_file_that_cannot_be_carried_is_skipped_but_english_refuses() {
    let temp = tempfile::tempdir().unwrap();
    let mut broken = german().into_bytes();
    broken.extend_from_slice(b"\xff");
    let archive = game(temp.path(), &[("citadel_gc_mod_names_english.txt", english().as_bytes()), ("citadel_gc_mod_names_german.txt", &broken)]);
    let receipt = item_names::stage(&archive, &[row("upgrade_plain_item", "Plainer")], &temp.path().join("out")).unwrap();
    assert_eq!(receipt.files.len(), 1);
    assert_eq!(receipt.skipped[0].file, "citadel_gc_mod_names_german.txt");
    assert!(receipt.notes().iter().any(|n| n.contains("german")));

    let other = tempfile::tempdir().unwrap();
    let twice = english().replace("\"modifier_twin\"", "\"upgrade_plain_item\"");
    let archive = game(other.path(), &[("citadel_gc_mod_names_english.txt", twice.as_bytes())]);
    let message = item_names::stage(&archive, &[row("upgrade_plain_item", "Plainer")], &other.path().join("out")).unwrap_err().to_string();
    assert!(message.contains(item_names::NOT_CARRIED) && message.contains("citadel_gc_mod_names_english.txt"), "{message}");
}

#[test]
fn a_missing_folder_or_item_is_refused_with_one_sentence() {
    let temp = tempfile::tempdir().unwrap();
    let nowhere = temp.path().join("pak01_dir.vpk");
    let message = item_names::stage(&nowhere, &[row("upgrade_plain_item", "A")], &temp.path().join("out")).unwrap_err().to_string();
    assert!(message.contains(item_names::NAMES_MISSING), "{message}");
    let stubs = tempfile::tempdir().unwrap();
    let archive = game(stubs.path(), &[("citadel_gc_mod_names_english.txt", STUB.as_bytes())]);
    assert!(item_names::stage(&archive, &[row("upgrade_plain_item", "A")], &stubs.path().join("out")).unwrap_err().to_string().contains(item_names::NAMES_MISSING));

    let archive = standard(temp.path());
    for item in ["upgrade_gone", "modifier_sample_regen", "upgrade_sample_rite_search", "language"] {
        let message = item_names::stage(&archive, &[row(item, "A")], &temp.path().join("out")).unwrap_err().to_string();
        assert!(message.contains(item_names::UNKNOWN_ITEM) && message.contains(item), "{message}");
    }
    // Analyze lists such rows instead of refusing.
    let inspected = item_names::inspect(&archive, &[row("upgrade_gone", "A"), row("upgrade_plain_item", "B")]).unwrap();
    assert_eq!(inspected.unknown, ["upgrade_gone"]);
    assert!(inspected.notes()[0].contains("upgrade_gone"));
    assert_eq!(inspected.files[0].changed, 1, "the known row still applies");
}

#[test]
fn the_install_check_refuses_a_changed_game_list_and_changed_files() {
    let temp = tempfile::tempdir().unwrap();
    let archive = standard(temp.path());
    let out = temp.path().join("out");
    let rows = [row("upgrade_sample_rite", "New Rite")];
    let receipt = item_names::stage(&archive, &rows, &out).unwrap();
    item_names::verify_entries(&archive, &receipt, &out).unwrap();
    // A packaged file that is not what the rows give.
    let english_out = out.join(&receipt.files[0].path);
    let good = std::fs::read(&english_out).unwrap();
    std::fs::write(&english_out, b"tampered").unwrap();
    assert!(item_names::verify_entries(&archive, &receipt, &out).unwrap_err().to_string().contains(item_names::ITEM_NAMES_CHANGED));
    std::fs::write(&english_out, &good).unwrap();
    // Built with another rule set (without the buff names rule).
    let mut other = receipt.clone();
    other.switches = WITHOUT_BUFFS;
    assert!(item_names::verify_entries(&archive, &other, &out).unwrap_err().to_string().contains(item_names::ITEM_NAMES_CHANGED));
    // A game update that changed the English list.
    let folder = item_names::folder(&archive);
    std::fs::write(folder.join("citadel_gc_mod_names_english.txt"), english().replace("\"Plain Item\"", "\"Plain Item 2\"")).unwrap();
    let changed = |receipt: &item_names::NamesReceipt| item_names::verify_entries(&archive, receipt, &out).unwrap_err().to_string();
    assert!(changed(&receipt).contains(item_names::GAME_NAMES_CHANGED), "{}", changed(&receipt));
    std::fs::write(folder.join("citadel_gc_mod_names_english.txt"), english()).unwrap();
    item_names::verify_entries(&archive, &receipt, &out).unwrap();
    // A stub that became a full file: one more language the build did not write.
    std::fs::write(folder.join("citadel_gc_mod_names_danish.txt"), german().replace("German", "danish")).unwrap();
    assert!(changed(&receipt).contains(item_names::GAME_NAMES_CHANGED));
    std::fs::write(folder.join("citadel_gc_mod_names_danish.txt"), STUB).unwrap();
    // A language file removed.
    std::fs::remove_file(folder.join("citadel_gc_mod_names_german.txt")).unwrap();
    assert!(changed(&receipt).contains(item_names::GAME_NAMES_CHANGED));
}

fn game_archive() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("DMB_PAK").expect("Set DMB_PAK to the game's pak01_dir.vpk"))
}

/// The one `*_dir.vpk` reference package in `subfolder` of the folder the
/// environment variable names.
fn reference_package(variable: &str, subfolder: &str) -> std::path::PathBuf {
    let folder = std::path::PathBuf::from(std::env::var(variable).unwrap_or_else(|_| panic!("Set {variable}"))).join(subfolder);
    let found: Vec<_> = std::fs::read_dir(&folder)
        .unwrap_or_else(|e| panic!("{}: {e}", folder.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.to_ascii_lowercase().ends_with("_dir.vpk")))
        .collect();
    assert_eq!(found.len(), 1, "one *_dir.vpk in {}: {found:?}", folder.display());
    found.into_iter().next().unwrap()
}

/// Compares the engine with a reference package made earlier with known
/// settings: the game's English names file with one item renamed, made without
/// the buff names rule, is exactly what the engine writes for that one rename,
/// with and without the rule (that item has no buff name). DMB_PAK,
/// DMB_ITEM_NAME_VPKS (the folder holding `2-whole-list` with the one package).
#[test]
#[ignore = "needs the game (DMB_PAK) and the reference packages (DMB_ITEM_NAME_VPKS)"]
fn the_whole_list_file_matches_the_reference_package() {
    let archive = game_archive();
    let test = deadlock_vpk::Vpk::open(reference_package("DMB_ITEM_NAME_VPKS", "2-whole-list")).unwrap();
    let path = "resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_english.txt";
    let expected = test.read(path).unwrap();
    for switches in [APP, WITHOUT_BUFFS] {
        let rendered = item_names::render(&item_names::folder(&archive), &[row("upgrade_close_range", "Test Name Two")], switches).unwrap();
        let english = rendered.receipt.files.iter().find(|f| f.language == "english").unwrap();
        assert_eq!(english.changed, 2, "{switches:?}");
        assert_eq!(rendered.outputs[&english.path], expected, "byte for byte, {switches:?}");
        println!("{switches:?}: {} language files written, {} skipped; English {} bytes equal", rendered.receipt.files.len(), rendered.receipt.skipped.len(), expected.len());
    }
}

/// Compares the engine with a reference package made earlier by a real build
/// with the buff names rule (two items renamed, their buff names with them):
/// the app's switches write every language file it holds, byte for byte, and
/// no other file. DMB_PAK, DMB_ITEM_NAME_BUILD_VPKS (the folder holding
/// `1-from-the-build` with the one package).
#[test]
#[ignore = "needs the game (DMB_PAK) and the reference package (DMB_ITEM_NAME_BUILD_VPKS)"]
fn the_build_file_matches_the_reference_package() {
    let archive = game_archive();
    let test = deadlock_vpk::Vpk::open(reference_package("DMB_ITEM_NAME_BUILD_VPKS", "1-from-the-build")).unwrap();
    let rows = [row("upgrade_health_stimpak", "Renamed Rite"), row("upgrade_magic_storm", "Renamed Surge")];
    let rendered = item_names::render(&item_names::folder(&archive), &rows, item_names::switches()).unwrap();
    assert_eq!(rendered.receipt.switches, Switches { buff_names: true });
    assert!(rendered.receipt.skipped.is_empty() && rendered.receipt.unknown.is_empty());
    let mut theirs: Vec<&str> = test.entries.values().map(|e| e.path.as_str()).collect();
    theirs.sort_unstable();
    let ours: Vec<&str> = rendered.outputs.keys().map(String::as_str).collect();
    assert_eq!(ours, theirs, "the same files");
    let mut bytes = 0;
    for file in &rendered.receipt.files {
        let expected = test.read(&file.path).unwrap();
        assert_eq!(rendered.outputs[&file.path], expected, "{} byte for byte", file.path);
        assert!(file.changed > 2, "{}: buff names renamed too ({})", file.path, file.changed);
        bytes += expected.len();
    }
    println!("{} language files equal byte for byte ({bytes} bytes)", rendered.receipt.files.len());
}

/// Every names file of the game reads with `Names` exactly as before, writes
/// back unchanged byte for byte, and the full ones hold the same tokens.
#[test]
#[ignore = "needs the game (DMB_PAK)"]
fn every_game_language_file_is_carried_exactly() {
    let folder = item_names::folder(&game_archive());
    let files = item_names::language_files(&folder).unwrap();
    assert!(files.len() >= 17, "{}", files.len());
    let english: Vec<String> = item_names::english(&folder).unwrap().keys().map(str::to_owned).collect();
    let mut full = 0;
    for (name, _) in &files {
        let raw = std::fs::read(folder.join(name)).unwrap();
        let text = std::str::from_utf8(&raw).unwrap();
        assert_eq!(new_names(text), old_names(text), "{name}");
        let file = NamesFile::read(&raw).unwrap();
        assert_eq!(file.write(&BTreeMap::new()).as_bytes(), raw.as_slice(), "{name}");
        if file.has_tokens() {
            full += 1;
            assert_eq!(file.keys().collect::<Vec<_>>(), english.iter().map(String::as_str).collect::<Vec<_>>(), "{name}");
        }
    }
    println!("{} files, {full} with tokens, {} English tokens", files.len(), english.len());
}

const HERO: &str = "models/heroes_staging/hornet_v3/hornet.vmdl";
/// A game folder for the components service: the shared component fixture as
/// `game/citadel/pak01_dir.vpk` and the made-up names files beside it.
fn service_game(root: &Path) -> std::path::PathBuf {
    let tree = root.join("game-source");
    let base = deadlock_vpk::Vpk::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/components_dir.vpk")).unwrap();
    let mut paths = vec![];
    for entry in base.entries.values() {
        deadlock_core::safety::atomic_write(&tree.join(&entry.path), base.read(&entry.path).unwrap()).unwrap();
        paths.push(entry.path.clone());
    }
    let pak = root.join("game/citadel/pak01_dir.vpk");
    std::fs::create_dir_all(pak.parent().unwrap()).unwrap();
    // The writer refuses the game's own name: written as another number and renamed.
    let spare = pak.with_file_name("pak42_dir.vpk");
    deadlock_vpk::write_vpk(&tree, &paths, &spare, None, 2, &mut || Ok(())).unwrap();
    std::fs::rename(&spare, &pak).unwrap();
    standard(&root.join("game/citadel"));
    pak
}

#[test]
fn renamed_items_build_freeze_and_install_with_and_without_a_replacement_model() {
    use deadlock_components::service::ComponentService;
    use deadlock_core::{CancellationToken, TimingRecorder, process::{Runner, RunnerOptions}};
    for model_enabled in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let pak = service_game(temp.path());
        let settings = deadlock_project::Settings {
            workspace: temp.path().join("workspace").to_string_lossy().into(),
            pak: pak.to_string_lossy().into(),
            ..Default::default()
        };
        let cancel = CancellationToken::new();
        let runner = Runner::new(temp.path().join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
        let clock = TimingRecorder::new("item names");
        let log = |_: &str| {};
        let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
        let mut project = deadlock_project::Project::default();
        project.original_model = HERO.into();
        project.customization.enabled = Some(true);
        project.customization.model_enabled = Some(model_enabled);
        project.customization.item_names = Some(vec![row("upgrade_sample_rite", "New Rite"), ItemNameRow { enabled: false, ..row("upgrade_gone", "Off") }]);
        // A switched-off row for an item the game does not have never refuses or shows.
        let inspection = service.inspect(&project).unwrap();
        assert_eq!(inspection.item_names.files.len(), 2);
        assert!(inspection.item_names.unknown.is_empty());
        let output = temp.path().join("output");
        let receipt = service.compile(&project, &output).unwrap().unwrap();
        let expected = [
            "resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_english.txt",
            "resource/localization/citadel_gc_mod_names/citadel_gc_mod_names_german.txt",
        ];
        assert_eq!(receipt.files.keys().collect::<Vec<_>>(), expected, "only the names files, never game files");
        assert!(receipt.original_resources.is_empty(), "loose files are not in the closure");
        assert_eq!(receipt.item_names.rows, [row("upgrade_sample_rite", "New Rite")]);
        assert_eq!(receipt.item_names.switches, item_names::SWITCHES);
        assert!(receipt.item_names.files.iter().all(|f| f.changed == 4), "the buff names are renamed by the app's build");
        service.verify_snapshot(&project, &receipt, &output).unwrap();
        let package = temp.path().join("pak18_dir.vpk");
        deadlock_vpk::write_vpk(&output, &receipt.files.keys().cloned().collect::<Vec<_>>(), &package, Some(&receipt.files), 2, &mut || Ok(())).unwrap();
        let candidate = deadlock_vpk::Vpk::open(&package).unwrap();
        let current = deadlock_vpk::Vpk::open(&pak).unwrap();
        service.verify_install(&project, &receipt, &candidate, &current).unwrap();
        // The receipt reads back from JSON.
        let back: deadlock_components::service::ComponentReceipt = serde_json::from_value(serde_json::to_value(&receipt).unwrap()).unwrap();
        assert_eq!(back.item_names, receipt.item_names);
        // Another name since the build: rebuild first.
        let mut renamed = project.clone();
        renamed.customization.item_names = Some(vec![row("upgrade_sample_rite", "Newer Rite")]);
        let message = service.verify_install(&renamed, &receipt, &candidate, &current).unwrap_err().to_string();
        assert!(message.contains(item_names::ITEM_NAMES_CHANGED), "{message}");
        // A game update that changed a names file: the install is refused.
        let folder = item_names::folder(&pak);
        std::fs::write(folder.join("citadel_gc_mod_names_german.txt"), german().replace("Schlichtes Ding", "Neues Ding")).unwrap();
        let message = service.verify_install(&project, &receipt, &candidate, &current).unwrap_err().to_string();
        assert!(message.contains(item_names::GAME_NAMES_CHANGED), "{message}");
        // An enabled row for an item the game does not have: listed by Analyze, refused by the build.
        std::fs::write(folder.join("citadel_gc_mod_names_german.txt"), german()).unwrap();
        let mut gone = project.clone();
        gone.customization.item_names = Some(vec![row("upgrade_gone", "Gone")]);
        let inspection = service.inspect(&gone).unwrap();
        assert_eq!(inspection.item_names.unknown, ["upgrade_gone"]);
        assert!(inspection.warnings.iter().any(|w| w.contains(item_names::UNKNOWN_ITEM)));
        let message = service.compile(&gone, &temp.path().join("output-2")).unwrap_err().to_string();
        assert!(message.contains(item_names::UNKNOWN_ITEM) && message.contains("upgrade_gone"), "{message}");
    }
}

#[test]
fn the_page_sees_the_game_s_names_and_which_rows_apply() {
    let temp = tempfile::tempdir().unwrap();
    let archive = standard(temp.path());
    let game = item_names::game_names(&archive);
    assert!(game.found);
    assert_eq!(game.languages, ["english", "german"], "stubs are not listed");
    assert_eq!(game.names["upgrade_sample_rite"], "Sample Rite");
    assert!(!game.names.contains_key("modifier_sample_regen") && !game.names.contains_key("upgrade_sample_rite_search"));
    assert_eq!(game.names.len(), 4);
    let states = item_names::row_states(&game, &[row("UPGRADE_PLAIN_ITEM", "A"), ItemNameRow { enabled: false, ..row("upgrade_gone", "B") }]);
    assert_eq!(states[0], item_names::RowState { item: "UPGRADE_PLAIN_ITEM".into(), game_name: Some("Plain Item".into()), known: true });
    assert_eq!(states[1], item_names::RowState { item: "upgrade_gone".into(), game_name: None, known: false });
    let stamp = item_names::folder_stamp(&archive);
    assert_eq!(stamp.len(), 3);
    assert!(!item_names::game_names(&temp.path().join("elsewhere/pak01_dir.vpk")).found);
}

#[test]
fn installed_names_files_say_how_many_of_the_game_s_names_they_lack() {
    let game = english();
    let older = game.replace("\t\t\"upgrade_plain_item\"\t\t\t\t\"Plain Item\"\r\n", "").replace("\t\t\"modifier_twin\"\t\t\"Twin\"\r\n", "");
    assert_eq!(item_names::missing_from(older.as_bytes(), game.as_bytes()), Some(2));
    assert_eq!(item_names::missing_from(game.as_bytes(), older.as_bytes()), Some(0), "extra names are not missing ones");
    assert_eq!(item_names::missing_from(b"\xff", game.as_bytes()), None);
    assert_eq!(item_names::outdated_note(&[("english".into(), 0)]), None);
    assert_eq!(
        item_names::outdated_note(&[("english".into(), 2), ("german".into(), 2)]).unwrap(),
        "2 of the game's item names are missing from its item names files (2 languages): those items show without a name until the mod is built again."
    );
    assert!(item_names::outdated_note(&[("english".into(), 1), ("german".into(), 3)]).unwrap().starts_with("Up to 3 "));
}
