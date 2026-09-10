// One cached copy of the providers.toml list for every chooser that offers
// "open an agent here": File ▸ New Tab, the blank-pane / empty-workspace
// state, the command palette. Warmed at menu init (`preloadProviderTabs`),
// re-warmed when the providers dialog saves or deletes
// (`invalidateProviderCache`). `Shell` is not a provider — callers append it.

import * as ipc from "../ipc";
import type { AIProvider } from "../types";

let cached: AIProvider[] | null = null;
let loading: Promise<void> | null = null;

/** Fetch the list once (shared in-flight promise); resolves when
 *  `getCachedProviderTabs()` is populated — or, on failure, still resolves
 *  so callers fall back to the built-ins. */
export function preloadProviderTabs(): Promise<void> {
  if (cached) return Promise.resolve();
  if (!loading) {
    loading = ipc.listProviders()
      .then((list) => {
        cached = list.map((p): AIProvider => ({ Custom: p.name }));
      })
      .catch((err) => {
        // Background warm-up: menus simply list the built-ins until the
        // next call retries.
        console.error("Failed to load providers:", err);
      })
      .finally(() => {
        loading = null;
      });
  }
  return loading;
}

/** Configured providers (`{ Custom }` each), or `[]` while not loaded yet —
 *  a miss kicks off the load so the next render has them. */
export function getCachedProviderTabs(): AIProvider[] {
  if (!cached) {
    void preloadProviderTabs();
    return [];
  }
  return cached;
}

/** Call after saving/deleting providers to refresh the cached list. */
export function invalidateProviderCache() {
  cached = null;
  void preloadProviderTabs();
}
