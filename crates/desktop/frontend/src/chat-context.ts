/**
 * Pure helpers for "Add context to chat" (phase 17): turn what the user is
 * looking at — a terminal selection, the active file, its git diff, an
 * editor selection — into a fenced block the chat composer can hold, and
 * parse tool messages back into card data. No DOM, no IPC; vitest-covered.
 */

export type ContextKind = "terminal" | "file" | "diff" | "editor-selection";

/** Max lines a single injected block may carry before it is cut. */
export const CONTEXT_MAX_LINES = 200;

export const TRUNCATED_MARKER = "…truncated";

export interface ContextMeta {
  /** Tab label (terminal) or workspace-relative path (file / diff / selection). */
  name: string;
  /** 1-based inclusive line range, when only part of a file is included. */
  lines?: { from: number; to: number };
}

/** Header line of a block, e.g. `File: src/main.rs (lines 10–42)`. */
export function contextHeader(kind: ContextKind, meta: ContextMeta): string {
  const range = meta.lines ? ` (lines ${meta.lines.from}–${meta.lines.to})` : "";
  switch (kind) {
    case "terminal":
      return `Terminal selection (tab "${meta.name}")`;
    case "file":
      return `File: ${meta.name}${range}`;
    case "diff":
      return `Diff: ${meta.name}`;
    case "editor-selection":
      return `Selected text in editor: ${meta.name}${range}`;
  }
}

/** Cut `text` to `max` lines, appending the truncation marker when it did. */
export function truncateLines(text: string, max = CONTEXT_MAX_LINES): { text: string; truncated: boolean } {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  if (lines.length <= max) return { text, truncated: false };
  return { text: lines.slice(0, max).join("\n") + "\n" + TRUNCATED_MARKER, truncated: true };
}

/** Fence language hint from a path's extension ("" when unknown). */
export function fenceLang(kind: ContextKind, name: string): string {
  if (kind === "diff") return "diff";
  if (kind === "terminal") return "text";
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return /^[a-z0-9]{1,10}$/.test(ext) && ext !== name.toLowerCase() ? ext : "";
}

/** A fence that cannot be closed early by the content: one more backtick
 *  than the longest run inside `text` (min 3). */
export function fenceFor(text: string): string {
  let longest = 0;
  for (const m of text.matchAll(/`+/g)) longest = Math.max(longest, m[0].length);
  return "`".repeat(Math.max(3, longest + 1));
}

/**
 * The block the composer receives: header line, then a fenced (possibly
 * truncated) body. Trailing whitespace of the body is dropped so the fence
 * closes tight; an empty body yields an empty string (nothing to inject).
 */
export function fenceBlock(kind: ContextKind, meta: ContextMeta, text: string): string {
  const body = text.replace(/\s+$/, "");
  if (body.length === 0) return "";
  const cut = truncateLines(body);
  const fence = fenceFor(cut.text);
  return `${contextHeader(kind, meta)}\n${fence}${fenceLang(kind, meta.name)}\n${cut.text}\n${fence}\n`;
}

/** Join a block onto the composer's current draft with a blank line between. */
export function appendToDraft(draft: string, block: string): string {
  if (!block) return draft;
  const head = draft.replace(/\s+$/, "");
  return head.length === 0 ? block : `${head}\n\n${block}`;
}

/** Flatten the backend's `DiffLine[]` (get_file_diff) to unified-diff text. */
export function diffLinesToText(lines: { content: string; line_type: string }[]): string {
  return lines
    .map((l) => {
      switch (l.line_type) {
        case "add":
          return `+${l.content}`;
        case "del":
          return `-${l.content}`;
        case "context":
          return ` ${l.content}`;
        default:
          return l.content;
      }
    })
    .join("\n");
}

// ── Tool cards ─────────────────────────────────────

export type ToolCardStatus = "running" | "ok" | "error" | "approval" | "approved" | "denied";

export interface ToolCard {
  id: string;
  name: string;
  /** Pretty-printed JSON arguments ("" when unknown). */
  args: string;
  result: string;
  status: ToolCardStatus;
  durationMs?: number;
}

/** Tool result content the backend stores in history: `[name] [Error] text`.
 *  Parsed back so a reloaded conversation still renders cards. */
export function parseToolMessage(content: string): { name: string; result: string; isError: boolean } {
  const m = /^\[([^\]\s]+)\] (\[Error\] )?([\s\S]*)$/.exec(content);
  if (!m) return { name: "tool", result: content, isError: false };
  return { name: m[1], result: m[3], isError: m[2] !== undefined };
}

/** Pretty JSON for a card body; unparsable input is returned as-is. */
export function prettyJson(value: unknown): string {
  if (typeof value === "string") {
    try {
      return JSON.stringify(JSON.parse(value), null, 2);
    } catch {
      return value;
    }
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/** Results longer than this many lines start collapsed ("Show more"). */
export const RESULT_COLLAPSE_LINES = 12;

export function formatDurationMs(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.floor(ms / 60_000)} m ${Math.round((ms % 60_000) / 1000)} s`;
}

/** The chat-context chooser: which entries are enabled for the current view. */
export interface ContextAvailability {
  terminalSelection: boolean;
  activeFile: boolean;
  editorSelection: boolean;
}

export function contextChoices(a: ContextAvailability): { kind: ContextKind; label: string; disabled: boolean }[] {
  return [
    { kind: "terminal", label: "Terminal selection", disabled: !a.terminalSelection },
    { kind: "file", label: "Active file", disabled: !a.activeFile },
    { kind: "diff", label: "Diff of active file", disabled: !a.activeFile },
    { kind: "editor-selection", label: "Selected text in editor", disabled: !a.editorSelection },
  ];
}
