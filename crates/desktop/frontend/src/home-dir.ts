// The user's home directory, read once at startup so chrome can print
// `~/git/x` instead of `/home/zero/git/x` (`labels.ts pathLabel`). Kept out
// of `labels.ts` on purpose: that module is pure and unit-tested in node,
// this one talks to Tauri.

import { homeDir } from "@tauri-apps/api/path";

let home: string | null = null;

/** The home directory, or `null` until `initHomeDir()` resolves (and for
 *  good if the platform refused to tell us — callers then print full paths). */
export function getHomeDir(): string | null {
  return home;
}

export async function initHomeDir(): Promise<void> {
  try {
    home = (await homeDir()).replace(/\/+$/, "") || null;
  } catch {
    home = null;
  }
}
