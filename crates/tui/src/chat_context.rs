//! Turning what the user is looking at — a terminal selection, a file — into
//! a fenced block the chat composer can hold. The TUI half of the desktop's
//! `chat-context.ts`: same header lines, same 200-line cap, same
//! can't-break-out fence, so a conversation reads identically whichever
//! frontend fed it. Pure — no `App`, no IO.

/// Max lines a single injected block may carry before it is cut.
pub(crate) const CONTEXT_MAX_LINES: usize = 200;

pub(crate) const TRUNCATED_MARKER: &str = "…truncated";

/// What the block holds; picks the header wording and the fence hint.
pub(crate) enum ContextKind {
    /// A selection in a terminal tab; `name` is the tab label.
    Terminal,
    /// A file; `name` is its workspace-relative path.
    File,
}

/// Header line of a block, e.g. `File: src/main.rs`.
pub(crate) fn context_header(kind: &ContextKind, name: &str) -> String {
    match kind {
        ContextKind::Terminal => format!("Terminal selection (tab \"{name}\")"),
        ContextKind::File => format!("File: {name}"),
    }
}

/// Cut `text` to `max` lines, appending the truncation marker when it did.
pub(crate) fn truncate_lines(text: &str, max: usize) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    if lines.len() <= max {
        return normalized;
    }
    format!("{}\n{TRUNCATED_MARKER}", lines[..max].join("\n"))
}

/// Fence language hint from a path's extension (empty when unknown).
fn fence_lang(kind: &ContextKind, name: &str) -> String {
    match kind {
        ContextKind::Terminal => "text".to_string(),
        ContextKind::File => match name.rsplit_once('.') {
            Some((stem, ext))
                if !stem.is_empty()
                    && (1..=10).contains(&ext.len())
                    && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
            {
                ext.to_ascii_lowercase()
            }
            _ => String::new(),
        },
    }
}

/// A fence that the content cannot close early: one more backtick than the
/// longest run inside `text` (minimum 3).
fn fence_for(text: &str) -> String {
    let mut longest = 0usize;
    let mut run = 0usize;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.saturating_add(1).max(3))
}

/// The block the composer receives: header line, then a fenced (possibly
/// truncated) body. Trailing whitespace is dropped so the fence closes tight;
/// an empty body yields an empty string — there is nothing to inject.
pub(crate) fn fence_block(kind: &ContextKind, name: &str, text: &str) -> String {
    let body = text.trim_end();
    if body.is_empty() {
        return String::new();
    }
    let cut = truncate_lines(body, CONTEXT_MAX_LINES);
    let fence = fence_for(&cut);
    format!(
        "{}\n{fence}{}\n{cut}\n{fence}\n",
        context_header(kind, name),
        fence_lang(kind, name),
    )
}

/// Join a block onto the composer's current draft with a blank line between.
pub(crate) fn append_to_draft(draft: &str, block: &str) -> String {
    if block.is_empty() {
        return draft.to_string();
    }
    let head = draft.trim_end();
    if head.is_empty() {
        block.to_string()
    } else {
        format!("{head}\n\n{block}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_wording_matches_the_desktop() {
        assert_eq!(
            context_header(&ContextKind::Terminal, "Claude Code"),
            "Terminal selection (tab \"Claude Code\")"
        );
        assert_eq!(
            context_header(&ContextKind::File, "src/main.rs"),
            "File: src/main.rs"
        );
    }

    #[test]
    fn a_short_body_is_kept_whole() {
        let out = truncate_lines("a\nb\nc", 200);
        assert_eq!(out, "a\nb\nc");
    }

    #[test]
    fn a_long_body_is_cut_and_marked() {
        let text = (0..250)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let out = truncate_lines(&text, 200);
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.len(), 201, "200 lines plus the marker");
        assert_eq!(lines[199], "199");
        assert_eq!(lines[200], TRUNCATED_MARKER);
    }

    #[test]
    fn crlf_is_normalized_before_counting() {
        assert_eq!(truncate_lines("a\r\nb", 200), "a\nb");
    }

    /// A body containing a fence must not be able to end the block early.
    #[test]
    fn the_fence_outgrows_the_content() {
        let block = fence_block(&ContextKind::Terminal, "sh", "```\ncode\n```");
        assert!(
            block.contains("````"),
            "expected a 4-backtick fence, got:\n{block}"
        );
    }

    #[test]
    fn an_empty_body_injects_nothing() {
        assert_eq!(fence_block(&ContextKind::Terminal, "sh", "   \n\n"), "");
        assert_eq!(append_to_draft("hi", ""), "hi");
    }

    #[test]
    fn a_file_block_carries_its_language_hint() {
        let block = fence_block(&ContextKind::File, "src/main.rs", "fn main() {}");
        assert!(block.starts_with("File: src/main.rs\n```rs\n"), "{block}");
        assert!(block.ends_with("```\n"), "{block}");
        // An extension-less file gets no hint.
        let plain = fence_block(&ContextKind::File, "Makefile", "all:");
        assert!(plain.contains("\n```\nall:"), "{plain}");
    }

    #[test]
    fn a_block_lands_after_what_was_already_typed() {
        let draft = append_to_draft("look at this", "File: a.rs\n```rs\nx\n```\n");
        assert_eq!(draft, "look at this\n\nFile: a.rs\n```rs\nx\n```\n");
        // Onto an empty composer it is the whole draft.
        assert_eq!(append_to_draft("  ", "block\n"), "block\n");
    }
}
