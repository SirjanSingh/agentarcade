//! Generated game packs: games written by the user's own coding agent and read
//! off disk at runtime, rather than compiled into the binary.
//!
//! A pack is a directory under `<app_data_dir>/games/`:
//!
//! ```text
//! games/semicolon-dodge/
//!   meta.json
//!   game.html
//! ```
//!
//! The directory name is the pack's id, and `meta.json` must repeat it. That
//! redundancy is deliberate: it means a pack cannot be renamed by moving its
//! folder, so the id in the user's stats always refers to the pack that
//! actually produced the score.
//!
//! Everything in here is written by a language model and is therefore
//! untrusted input. `game.html` is never executed in the app's own context —
//! it is served over the `agpack:` protocol into a sandboxed iframe with its
//! own CSP (see `main.rs`). This module's job is the layer before that: decide
//! which packs are well-formed enough to load at all, and never let a
//! malformed or hostile one affect the others.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Cap on `game.html`. The contract asks for one mechanic in roughly 150
/// lines; anything approaching this is not that, and the whole file is read
/// into memory to be served.
const MAX_HTML_BYTES: u64 = 256 * 1024;

/// Longest acceptable pack id. Long enough to be descriptive, short enough
/// that it cannot be used to push a path near a filesystem limit.
const MAX_ID_LEN: usize = 40;

/// The URI scheme generated games are served over. Registered in `main.rs` and
/// allowed by `frame-src` in the app's CSP.
pub const SCHEME: &str = "agpack";

/// The URL the webview must use to load a pack's `game.html`.
///
/// WebView2 maps a custom scheme onto `http://<scheme>.localhost`, while the
/// macOS and Linux webviews keep `<scheme>://localhost`. Deciding it here
/// rather than in the frontend keeps the platform difference in the one module
/// that already knows how the protocol is served.
pub fn asset_url(id: &str) -> String {
    if cfg!(windows) {
        format!("http://{SCHEME}.localhost/{id}/game.html")
    } else {
        format!("{SCHEME}://localhost/{id}/game.html")
    }
}

/// One valid pack. Field names mirror `GameModule` in `src/game.ts` so the
/// frontend can build a game module from this without translation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub id: String,
    pub label: String,
    /// e.g. "pts", "ms" — rendered next to the score.
    pub unit: String,
    pub lower_is_better: bool,
    /// What the user typed to get this game. Provenance: it is what makes a
    /// pack reviewable if it is ever offered back as a contribution.
    pub prompt: String,
    pub created_at: String,
    /// Where the frontend loads this game from. Derived, not stored on disk.
    pub url: String,
}

/// True if `id` is safe to use as a directory name and a URL segment.
///
/// This is the traversal guard. It is deliberately a strict allowlist rather
/// than a check for `..` or separators: an id reaches the filesystem from two
/// directions — a directory name found by a scan, and a string handed back by
/// the webview — and only one of those is under our control.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Absolute path to a pack's `game.html`, or `None` if `id` is not a valid
/// pack id or the file is not there.
///
/// Callers must route every filesystem access through this rather than joining
/// an id onto a path themselves.
pub fn html_path(dir: &Path, id: &str) -> Option<PathBuf> {
    if !valid_id(id) {
        return None;
    }
    let path = dir.join(id).join("game.html");
    path.is_file().then_some(path)
}

/// Every well-formed pack in `dir`, sorted by id so the rotation order does
/// not depend on how the filesystem happens to enumerate directories.
///
/// A pack that fails any check is skipped and logged. Nothing in here returns
/// an error: one bad pack must never stop the good ones loading, and a missing
/// or unreadable games directory just means there are no packs yet.
pub fn scan(dir: &Path, builtins: &[String]) -> Vec<Pack> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut packs: Vec<Pack> = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        match load_one(dir, &id, builtins, &packs) {
            Ok(pack) => packs.push(pack),
            Err(reason) => alog!("skipping game pack {id:?}: {reason}"),
        }
    }

    packs.sort_by(|a, b| a.id.cmp(&b.id));
    packs
}

/// Validate one directory. `accepted` is what has been taken so far, so a
/// second pack claiming an id already in use loses.
fn load_one(dir: &Path, id: &str, builtins: &[String], accepted: &[Pack]) -> Result<Pack, String> {
    if !valid_id(id) {
        return Err("the folder name must be 1-40 characters of a-z, 0-9 and -".into());
    }
    if builtins.iter().any(|b| b == id) {
        return Err("that id belongs to a built-in game".into());
    }
    if accepted.iter().any(|p| p.id == id) {
        return Err("another pack already claimed that id".into());
    }

    let html = dir.join(id).join("game.html");
    let meta = fs::read_to_string(dir.join(id).join("meta.json"))
        .map_err(|err| format!("meta.json could not be read: {err}"))?;
    // Windows agents reach for PowerShell, whose Out-File and Set-Content write
    // UTF-8 *with* a byte order mark. serde_json rejects one, and the pack is
    // then skipped for a reason nobody can see by looking at the file. Strip it
    // rather than make every generator know this.
    let meta: serde_json::Value = serde_json::from_str(meta.trim_start_matches('\u{feff}'))
        .map_err(|err| format!("meta.json is not valid JSON: {err}"))?;

    let size = fs::metadata(&html)
        .map_err(|err| format!("game.html could not be read: {err}"))?
        .len();
    if size > MAX_HTML_BYTES {
        return Err(format!("game.html is {size} bytes, over the {MAX_HTML_BYTES} limit"));
    }

    let text = |key: &str| -> Result<String, String> {
        meta.get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("meta.json has no non-empty {key:?}"))
    };

    if text("id")? != id {
        return Err("meta.json \"id\" does not match the folder name".into());
    }

    Ok(Pack {
        id: id.to_owned(),
        label: text("label")?,
        unit: text("unit")?,
        lower_is_better: meta
            .get("lowerIsBetter")
            .and_then(serde_json::Value::as_bool)
            .ok_or("meta.json has no boolean \"lowerIsBetter\"")?,
        prompt: text("prompt")?,
        created_at: text("createdAt")?,
        url: asset_url(id),
    })
}

/// Remove a pack's directory. Unlike `scan`, a failure here is visible: the
/// user pressed a delete button and is owed an answer.
pub fn delete(dir: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("not a valid pack id".into());
    }
    let target = dir.join(id);
    if !target.is_dir() {
        return Err("that game is not installed".into());
    }
    fs::remove_dir_all(&target).map_err(|err| format!("could not delete the game: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtins() -> Vec<String> {
        vec!["snake".to_string(), "typing".to_string()]
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentarcade-packs-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn meta_for(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","label":"Dodge","unit":"pts","lowerIsBetter":false,
                "prompt":"dodge semicolons","createdAt":"2026-09-06T11:02:44Z"}}"#
        )
    }

    /// Write a well-formed pack, then let the caller break one part of it.
    fn write_pack(dir: &Path, id: &str, meta: &str, html: &str) {
        let pack = dir.join(id);
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("meta.json"), meta).unwrap();
        fs::write(pack.join("game.html"), html).unwrap();
    }

    #[test]
    fn a_well_formed_pack_is_found() {
        let dir = temp_dir("valid");
        write_pack(&dir, "semicolon-dodge", &meta_for("semicolon-dodge"), "<canvas>");

        let packs = scan(&dir, &builtins());
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].id, "semicolon-dodge");
        assert_eq!(packs[0].label, "Dodge");
        assert_eq!(packs[0].unit, "pts");
        assert!(!packs[0].lower_is_better);
        assert_eq!(packs[0].prompt, "dodge semicolons");
    }

    #[test]
    fn a_missing_games_directory_is_not_an_error() {
        let dir = temp_dir("missing").join("does-not-exist");
        assert!(scan(&dir, &builtins()).is_empty());
    }

    #[test]
    fn one_bad_pack_does_not_hide_a_good_one() {
        let dir = temp_dir("mixed");
        write_pack(&dir, "good-one", &meta_for("good-one"), "<canvas>");
        write_pack(&dir, "bad-one", "{ not json", "<canvas>");

        let packs = scan(&dir, &builtins());
        assert_eq!(packs.len(), 1, "the malformed pack must not take the other down");
        assert_eq!(packs[0].id, "good-one");
    }

    #[test]
    fn malformed_json_is_skipped() {
        let dir = temp_dir("badjson");
        write_pack(&dir, "broken", "{ not json", "<canvas>");
        assert!(scan(&dir, &builtins()).is_empty());
    }

    #[test]
    fn a_byte_order_mark_does_not_reject_a_pack() {
        // PowerShell's Out-File writes one by default, so this is the likeliest
        // way a correctly followed instruction still produces a dead pack.
        let dir = temp_dir("bom");
        let with_bom = format!("\u{feff}{}", meta_for("marked"));
        write_pack(&dir, "marked", &with_bom, "<canvas>");

        let packs = scan(&dir, &builtins());
        assert_eq!(packs.len(), 1, "a BOM is not a malformed file");
        assert_eq!(packs[0].id, "marked");
    }

    #[test]
    fn a_missing_field_is_skipped() {
        let dir = temp_dir("nofield");
        write_pack(
            &dir,
            "partial",
            r#"{"id":"partial","label":"X","unit":"pts","prompt":"p","createdAt":"now"}"#,
            "<canvas>",
        );
        assert!(scan(&dir, &builtins()).is_empty(), "lowerIsBetter is required");
    }

    #[test]
    fn an_empty_field_is_skipped() {
        let dir = temp_dir("emptyfield");
        write_pack(
            &dir,
            "blank",
            r#"{"id":"blank","label":"  ","unit":"pts","lowerIsBetter":false,
                "prompt":"p","createdAt":"now"}"#,
            "<canvas>",
        );
        assert!(scan(&dir, &builtins()).is_empty(), "a blank label is not a label");
    }

    #[test]
    fn a_meta_id_that_disagrees_with_the_folder_is_skipped() {
        let dir = temp_dir("iddrift");
        write_pack(&dir, "on-disk", &meta_for("in-the-file"), "<canvas>");
        assert!(scan(&dir, &builtins()).is_empty());
    }

    #[test]
    fn a_pack_cannot_shadow_a_built_in_game() {
        let dir = temp_dir("collide");
        write_pack(&dir, "snake", &meta_for("snake"), "<canvas>");
        assert!(scan(&dir, &builtins()).is_empty());
    }

    #[test]
    fn a_missing_game_html_is_skipped() {
        let dir = temp_dir("nohtml");
        let pack = dir.join("htmlless");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("meta.json"), meta_for("htmlless")).unwrap();
        assert!(scan(&dir, &builtins()).is_empty());
    }

    #[test]
    fn an_oversized_game_html_is_skipped() {
        let dir = temp_dir("toobig");
        let huge = "x".repeat(MAX_HTML_BYTES as usize + 1);
        write_pack(&dir, "enormous", &meta_for("enormous"), &huge);
        assert!(scan(&dir, &builtins()).is_empty());
    }

    #[test]
    fn ids_the_filesystem_allows_but_we_do_not_are_skipped() {
        let dir = temp_dir("badids");
        // Uppercase and underscores are legal directory names on every platform
        // we ship to, so the scan has to reject them itself.
        write_pack(&dir, "Shouty", &meta_for("Shouty"), "<canvas>");
        write_pack(&dir, "under_score", &meta_for("under_score"), "<canvas>");
        assert!(scan(&dir, &builtins()).is_empty());
    }

    #[test]
    fn packs_come_back_in_a_stable_order() {
        let dir = temp_dir("order");
        for id in ["zebra", "apple", "mango"] {
            write_pack(&dir, id, &meta_for(id), "<canvas>");
        }
        let ids: Vec<String> = scan(&dir, &builtins()).into_iter().map(|p| p.id).collect();
        assert_eq!(ids, vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn valid_id_rejects_traversal_and_separators() {
        assert!(valid_id("semicolon-dodge"));
        assert!(!valid_id(""));
        assert!(!valid_id(".."));
        assert!(!valid_id("../../etc/passwd"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id("a\\b"));
        assert!(!valid_id("C:"));
        assert!(!valid_id(&"a".repeat(MAX_ID_LEN + 1)));
    }

    #[test]
    fn html_path_refuses_a_traversal_id_handed_straight_to_it() {
        let dir = temp_dir("htmlpath");
        write_pack(&dir, "real-game", &meta_for("real-game"), "<canvas>");

        assert!(html_path(&dir, "real-game").is_some());
        // The webview supplies this id, so it is checked here and not only
        // where the scan happens to have found it.
        assert!(html_path(&dir, "../real-game").is_none());
        assert!(html_path(&dir, "nope").is_none());
    }

    #[test]
    fn delete_removes_only_a_valid_installed_pack() {
        let dir = temp_dir("delete");
        write_pack(&dir, "goner", &meta_for("goner"), "<canvas>");

        assert!(delete(&dir, "../goner").is_err(), "traversal must not delete");
        assert!(delete(&dir, "never-installed").is_err());
        assert!(delete(&dir, "goner").is_ok());
        assert!(!dir.join("goner").exists());
        assert!(delete(&dir, "goner").is_err(), "deleting twice must report it");
    }
}
