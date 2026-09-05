//! Installing the skill that generates games.
//!
//! agentarcade cannot write a game itself — it has no model and no network.
//! What it can do is put a skill where the user's coding agent will find it,
//! so `/agentarcade:make dodge falling semicolons` works in the terminal they
//! are already sitting in.
//!
//! This writes outside the app's own data directory, into `~/.claude/skills/`,
//! so it follows the same discipline as the hooks installer in `tray.rs`: it
//! happens only when the user asks, it leaves a marker so uninstalling removes
//! exactly what was added, and it refuses rather than overwriting a directory
//! it did not create.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// The skill's directory name under `~/.claude/skills/`.
pub const SKILL_DIR: &str = "agentarcade-make";

/// Written beside the skill so uninstall can tell our directory from one the
/// user wrote themselves. Its contents are for a human reading the folder.
const MARKER: &str = ".installed-by-agentarcade";
const MARKER_BODY: &str =
    "This folder was written by agentarcade's settings window and is removed when\n\
     the game-maker command is turned off there. Delete this file if you want to\n\
     keep the skill and manage it yourself.\n";

const SKILL_MD: &str = include_str!("../skill/SKILL.md");
const REFERENCE_MD: &str = include_str!("../skill/REFERENCE.md");

/// What the settings window needs to decide which button to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillStatus {
    /// Not there. Offer to install it.
    Absent,
    /// Ours. Offer to remove it.
    Installed,
    /// Something else is using that name. Touch nothing and say so.
    Foreign,
}

fn marker_path(skill_dir: &Path) -> PathBuf {
    skill_dir.join(MARKER)
}

pub fn status(skill_dir: &Path) -> SkillStatus {
    if !skill_dir.exists() {
        SkillStatus::Absent
    } else if marker_path(skill_dir).is_file() {
        SkillStatus::Installed
    } else {
        SkillStatus::Foreign
    }
}

/// Write the skill. Fails rather than clobbering anything we did not write.
///
/// Reinstalling over our own copy is allowed and is how an upgraded app
/// refreshes the contract text after the sandbox rules change.
pub fn install(skill_dir: &Path) -> Result<(), String> {
    if status(skill_dir) == SkillStatus::Foreign {
        return Err(format!(
            "{} already exists and was not created by agentarcade, so it has been left alone.",
            skill_dir.display()
        ));
    }

    let write = |name: &str, body: &str| -> Result<(), String> {
        crate::atomic_write::write(&skill_dir.join(name), body.as_bytes())
            .map_err(|err| format!("could not write {name}: {err}"))
    };

    // The marker goes last: if writing the skill fails halfway, what is left
    // behind is unmarked and a later install will refuse rather than silently
    // repairing a directory whose contents nobody has checked.
    write("SKILL.md", SKILL_MD)?;
    write("REFERENCE.md", REFERENCE_MD)?;
    write(MARKER, MARKER_BODY)?;
    Ok(())
}

/// Remove the skill, but only if we put it there.
pub fn remove(skill_dir: &Path) -> Result<(), String> {
    match status(skill_dir) {
        SkillStatus::Absent => Ok(()),
        SkillStatus::Foreign => Err(format!(
            "{} was not created by agentarcade, so it has been left alone.",
            skill_dir.display()
        )),
        SkillStatus::Installed => fs::remove_dir_all(skill_dir)
            .map_err(|err| format!("could not remove the skill: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentarcade-maker-{name}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_absent_skill_installs_and_reports_itself() {
        let dir = temp_dir("install");
        assert_eq!(status(&dir), SkillStatus::Absent);

        install(&dir).unwrap();

        assert_eq!(status(&dir), SkillStatus::Installed);
        assert!(dir.join("SKILL.md").is_file());
        assert!(dir.join("REFERENCE.md").is_file());
    }

    #[test]
    fn the_shipped_skill_carries_its_frontmatter_and_contract() {
        // include_str! makes these compile-time constants, so this is really a
        // check that the files next to the binary are still the right files.
        assert!(SKILL_MD.starts_with("---\nname: agentarcade-make\n"));
        assert!(REFERENCE_MD.contains("agentarcade:score"));
        assert!(REFERENCE_MD.contains("agentarcade:freeze"));
    }

    #[test]
    fn reinstalling_over_our_own_copy_is_allowed() {
        let dir = temp_dir("reinstall");
        install(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "stale").unwrap();

        install(&dir).unwrap();

        assert_ne!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), "stale");
    }

    #[test]
    fn a_directory_we_did_not_write_is_never_touched() {
        let dir = temp_dir("foreign");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "the user's own skill").unwrap();

        assert_eq!(status(&dir), SkillStatus::Foreign);
        assert!(install(&dir).is_err());
        assert!(remove(&dir).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("SKILL.md")).unwrap(),
            "the user's own skill",
            "refusing must mean refusing, not overwriting"
        );
    }

    #[test]
    fn remove_takes_our_copy_and_is_safe_to_repeat() {
        let dir = temp_dir("remove");
        install(&dir).unwrap();

        remove(&dir).unwrap();
        assert!(!dir.exists());
        assert!(remove(&dir).is_ok(), "removing what is already gone is not an error");
    }
}
