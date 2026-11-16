//! Offers a repo's coding agents (any of them — Claude Code, Codex,
//! muse-cli, whatever reads `AGENTS.md`) a documented, versioned set of
//! piki capabilities, by injecting a marked block into that repo's
//! `AGENTS.md`. The user opts in per repo, per skill — nothing here writes
//! to a repo's files without an explicit accept recorded in
//! [`crate::storage::RepoSkillStorage`].
//!
//! A [`Skill`] is static content shipped with piki (a new skill = a new
//! entry in [`SKILLS`] plus a markdown file under `skills/`), not something
//! a user authors — only the *adoption decision* (accepted/declined, and at
//! which [`Skill::version`]) is per-repo state, in SQLite. Bumping a skill's
//! `version` makes [`pending_offers`] re-offer it to repos that already
//! decided on an older version, without re-prompting ones that already
//! accepted or declined the current one.
//!
//! The injected block is idempotent: wrapped in [`MARKER_START`] /
//! [`MARKER_END`], so re-running [`sync_accepted_skills`] (new skill
//! accepted, or an accepted skill's content bumped) replaces exactly that
//! block and leaves the rest of the file — including anything the user
//! wrote around it — untouched.

use std::path::Path;

use crate::storage::RepoSkillStorage;

#[derive(Debug)]
pub struct Skill {
    /// Stable id — the primary key `RepoSkillStorage` tracks decisions
    /// under. Never reuse an id for different content; bump `version`
    /// instead.
    pub id: &'static str,
    /// Bump when `markdown` changes meaningfully enough to re-offer the
    /// skill to repos that already decided on an older version.
    pub version: u32,
    pub title: &'static str,
    /// Injected verbatim into the AGENTS.md block, as its own section.
    pub markdown: &'static str,
}

pub const SKILLS: &[Skill] = &[Skill {
    id: "piki-ctl-open-file",
    version: 1,
    title: "piki-ctl — open files in piki's UI",
    markdown: include_str!("skills/piki_ctl_open_file.md"),
}];

const MARKER_START: &str = "<!-- piki:skills:start -->";
const MARKER_END: &str = "<!-- piki:skills:end -->";

/// Skills this repo hasn't decided on at their current version — what a
/// "add piki skills to AGENTS.md?" prompt should offer. Storage errors are
/// treated as "don't nag": a skill is only ever offered when we can
/// positively confirm it wasn't already decided.
pub fn pending_offers<'a>(storage: &dyn RepoSkillStorage, source_repo: &Path) -> Vec<&'a Skill> {
    SKILLS
        .iter()
        .filter(|s| match storage.get_skill_offer(source_repo, s.id) {
            Ok(Some((decided_version, _status))) => decided_version < s.version,
            Ok(None) => true,
            Err(_) => false,
        })
        .collect()
}

/// Rebuild the injected block from every skill currently `"accepted"` for
/// `source_repo` and write it into `agents_md_path` (created if missing).
/// Call after recording a new accept/decline, or when a skill's version
/// bumps a previously-accepted one — always resyncs the *whole* block from
/// the full accepted set, never just the skill that just changed, so the
/// file never drifts from `RepoSkillStorage`.
pub fn sync_accepted_skills(
    storage: &dyn RepoSkillStorage,
    source_repo: &Path,
    agents_md_path: &Path,
) -> anyhow::Result<()> {
    let accepted_ids = storage.list_accepted_skills(source_repo)?;
    let accepted: Vec<&Skill> = SKILLS
        .iter()
        .filter(|s| accepted_ids.iter().any(|id| id == s.id))
        .collect();

    let existing = std::fs::read_to_string(agents_md_path).unwrap_or_default();
    let updated = if accepted.is_empty() {
        remove_block(&existing)
    } else {
        upsert_block(&existing, &render_block(&accepted))
    };
    if updated != existing {
        std::fs::write(agents_md_path, updated)?;
    }
    Ok(())
}

fn render_block(skills: &[&Skill]) -> String {
    let body: String = skills
        .iter()
        .map(|s| s.markdown.trim_end())
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "{MARKER_START}\n\
         <!-- Managed by piki — edits inside this block are overwritten the\n\
         next time a skill here changes. Decline a skill from piki's repo\n\
         settings to remove it instead of editing here. -->\n\n\
         {body}\n\
         {MARKER_END}"
    )
}

/// Replace an existing marked block in `existing` with `block`, or append
/// `block` (with a blank-line separator) if no block is present yet.
fn upsert_block(existing: &str, block: &str) -> String {
    match (existing.find(MARKER_START), existing.find(MARKER_END)) {
        (Some(start), Some(end)) if end > start => {
            let end = end + MARKER_END.len();
            format!("{}{}{}", &existing[..start], block, &existing[end..])
        }
        _ => {
            let trimmed = existing.trim_end_matches('\n');
            if trimmed.is_empty() {
                format!("{block}\n")
            } else {
                format!("{trimmed}\n\n{block}\n")
            }
        }
    }
}

/// Strip a previously-injected block entirely (every accepted skill was
/// declined/removed) rather than leaving an empty marker pair behind.
fn remove_block(existing: &str) -> String {
    match (existing.find(MARKER_START), existing.find(MARKER_END)) {
        (Some(start), Some(end)) if end > start => {
            let end = end + MARKER_END.len();
            let before = existing[..start].trim_end_matches('\n');
            let after = existing[end..].trim_start_matches('\n');
            match (before.is_empty(), after.is_empty()) {
                (true, true) => String::new(),
                (true, false) => after.to_string(),
                (false, true) => format!("{before}\n"),
                (false, false) => format!("{before}\n\n{after}"),
            }
        }
        _ => existing.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_appends_block_to_file_with_no_marker() {
        let out = upsert_block(
            "# My repo\n\nSome notes.\n",
            "<!-- piki:skills:start -->\nX\n<!-- piki:skills:end -->",
        );
        assert_eq!(
            out,
            "# My repo\n\nSome notes.\n\n<!-- piki:skills:start -->\nX\n<!-- piki:skills:end -->\n"
        );
    }

    #[test]
    fn upsert_appends_block_to_empty_file() {
        let out = upsert_block(
            "",
            "<!-- piki:skills:start -->\nX\n<!-- piki:skills:end -->",
        );
        assert_eq!(
            out,
            "<!-- piki:skills:start -->\nX\n<!-- piki:skills:end -->\n"
        );
    }

    #[test]
    fn upsert_replaces_existing_block_in_place_leaving_surrounding_content() {
        let existing = "# My repo\n\n<!-- piki:skills:start -->\nold\n<!-- piki:skills:end -->\n\nMy own notes below.\n";
        let out = upsert_block(
            existing,
            "<!-- piki:skills:start -->\nnew\n<!-- piki:skills:end -->",
        );
        assert_eq!(
            out,
            "# My repo\n\n<!-- piki:skills:start -->\nnew\n<!-- piki:skills:end -->\n\nMy own notes below.\n"
        );
    }

    #[test]
    fn remove_block_strips_marker_pair_and_normalizes_blank_lines() {
        let existing = "# My repo\n\n<!-- piki:skills:start -->\nX\n<!-- piki:skills:end -->\n\nMy own notes.\n";
        let out = remove_block(existing);
        assert_eq!(out, "# My repo\n\nMy own notes.\n");
    }

    #[test]
    fn remove_block_is_a_no_op_without_a_marker() {
        let existing = "# My repo\n\nMy own notes.\n";
        assert_eq!(remove_block(existing), existing);
    }

    #[test]
    fn render_block_joins_multiple_skills_with_a_managed_notice() {
        let a = Skill {
            id: "a",
            version: 1,
            title: "A",
            markdown: "Body A",
        };
        let b = Skill {
            id: "b",
            version: 1,
            title: "B",
            markdown: "Body B",
        };
        let block = render_block(&[&a, &b]);
        assert!(block.starts_with(MARKER_START));
        assert!(block.ends_with(MARKER_END));
        assert!(block.contains("Body A"));
        assert!(block.contains("Body B"));
        assert!(block.contains("Managed by piki"));
    }
}
