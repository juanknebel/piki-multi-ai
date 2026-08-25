// Pure helpers behind Settings ▸ Shortcuts (no DOM, vitest-covered):
// grouping by category, the filter box, and conflict detection. The dialog
// only renders what these return; `shortcuts.ts` stays the registry.

/** The slice of a `ShortcutDef` these helpers look at. */
export interface ShortcutLike {
  id: string;
  label: string;
  category: string;
  key: string;
}

/** A key a rebind may not claim (widget bindings) — `RESERVED_COMBOS` in shortcuts.ts. */
export interface ReservedLike {
  key: string;
  label: string;
}

/** Combos compare case-insensitively ("Ctrl+a" and "Ctrl+A" are one key). */
export function normalizeCombo(combo: string): string {
  return combo.trim().toLowerCase();
}

/**
 * Every shortcut whose current key is also bound elsewhere → the labels of
 * the other bindings (other shortcuts first, then reserved widget keys).
 * A def with an empty key never conflicts. The registry blocks a rebind
 * onto a taken key, so conflicts come from persisted overrides colliding
 * with a default added later — exactly what the dialog needs to surface.
 */
export function findConflicts(
  defs: readonly ShortcutLike[],
  reserved: readonly ReservedLike[] = [],
): Map<string, string[]> {
  const byCombo = new Map<string, ShortcutLike[]>();
  for (const def of defs) {
    const combo = normalizeCombo(def.key);
    if (!combo) continue;
    const list = byCombo.get(combo);
    if (list) list.push(def);
    else byCombo.set(combo, [def]);
  }
  const reservedByCombo = new Map<string, string[]>();
  for (const r of reserved) {
    const combo = normalizeCombo(r.key);
    const list = reservedByCombo.get(combo);
    if (list) list.push(r.label);
    else reservedByCombo.set(combo, [r.label]);
  }

  const out = new Map<string, string[]>();
  for (const def of defs) {
    const combo = normalizeCombo(def.key);
    if (!combo) continue;
    const others = (byCombo.get(combo) ?? []).filter((d) => d.id !== def.id).map((d) => d.label);
    const widgets = reservedByCombo.get(combo) ?? [];
    const all = [...others, ...widgets];
    if (all.length > 0) out.set(def.id, all);
  }
  return out;
}

/**
 * Case-insensitive, every whitespace-separated word must match the label,
 * the key (raw or platform-formatted, both are passed) or the category.
 * An empty query keeps everything.
 */
export function matchesShortcutQuery(
  def: ShortcutLike,
  query: string,
  formattedKey: string = def.key,
): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const hay = `${def.label}\n${def.key}\n${formattedKey}\n${def.category}`.toLowerCase();
  return words.every((w) => hay.includes(w));
}

/**
 * Group in `order`, keeping the registry's order inside each group; groups
 * that end up empty (nothing matched) are dropped. Categories not in
 * `order` go last, in first-seen order, so a registry typo still renders.
 */
export function groupByCategory<T extends ShortcutLike>(
  defs: readonly T[],
  order: readonly string[],
): { category: string; items: T[] }[] {
  const groups = new Map<string, T[]>();
  for (const c of order) groups.set(c, []);
  for (const def of defs) {
    const list = groups.get(def.category);
    if (list) list.push(def);
    else groups.set(def.category, [def]);
  }
  return [...groups.entries()]
    .filter(([, items]) => items.length > 0)
    .map(([category, items]) => ({ category, items }));
}
