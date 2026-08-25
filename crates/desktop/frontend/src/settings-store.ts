// Single owner of the desktop's `settings` JSON document (the `settings`
// row in `UiPrefsStorage`, fetched via `get_settings` / `set_settings`).
//
// Before this store every caller did its own read-modify-write of the whole
// document (`getSettings` → JSON.parse → tweak → `setSettings`). Two of
// those racing — a sidebar drag landing while the pane-layout flush is
// mid-flight — meant the later writer clobbered the earlier one's key. Now
// the document lives in memory once, `patch()` mutates that snapshot
// synchronously, and one debounced writer serializes the whole thing.
//
// Rules for callers:
// - `await settingsStore.load()` once at startup (main.ts does), before any
//   `get()`. `get()` never hits IPC.
// - `patch(key, value)` for every write; never call `ipc.setSettings`
//   directly. Passing `undefined` deletes the key.
// - The document is also read by the Rust side (`commands/pty.rs` reads
//   `shell`), so it always goes out whole — no partial writes.

export interface SettingsBackend {
  read(): Promise<string | null>;
  write(json: string): Promise<void>;
}

export type SettingsDoc = Record<string, unknown>;

export interface SettingsStore {
  /** Fetch the document once; concurrent/repeat calls share the first load. */
  load(): Promise<void>;
  /** True once `load()` has resolved (successfully or not). */
  readonly loaded: boolean;
  /** Read a key from the in-memory snapshot. `undefined` when absent. */
  get<T = unknown>(key: string): T | undefined;
  /** Read-only view of the whole document. */
  snapshot(): Readonly<SettingsDoc>;
  /** Set (or delete with `undefined`) a key and schedule one debounced write. */
  patch(key: string, value: unknown): void;
  /** Write now if anything is pending; resolves when the backend accepted it. */
  flush(): Promise<void>;
}

export interface SettingsStoreOptions {
  debounceMs?: number;
  /** Called when a write fails; the snapshot keeps the patched values so the
   *  next successful write carries them. Defaults to `console.error`. */
  onError?: (err: unknown) => void;
}

export function createSettingsStore(
  backend: SettingsBackend,
  { debounceMs = 300, onError = (e) => console.error("settings write failed:", e) }: SettingsStoreOptions = {},
): SettingsStore {
  let doc: SettingsDoc = {};
  let loaded = false;
  let loading: Promise<void> | null = null;
  let dirty = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let inflight: Promise<void> | null = null;

  async function doLoad() {
    try {
      const raw = await backend.read();
      const parsed = raw ? JSON.parse(raw) : {};
      // A load that lands after an early `patch()` must not drop the patch.
      doc = parsed && typeof parsed === "object" && !Array.isArray(parsed) ? { ...parsed, ...doc } : { ...doc };
    } catch {
      // Unreadable/corrupt document: start from whatever was patched so far.
    } finally {
      loaded = true;
    }
  }

  async function writeNow(): Promise<void> {
    // Serialize writes: a patch arriving during a write re-arms `dirty` and
    // the loop below picks it up, so the backend always ends with the
    // latest snapshot and never sees two writes interleave.
    if (inflight) return inflight;
    inflight = (async () => {
      while (dirty) {
        dirty = false;
        const json = JSON.stringify(doc);
        try {
          await backend.write(json);
        } catch (err) {
          dirty = true;
          onError(err);
          break;
        }
      }
    })().finally(() => {
      inflight = null;
    });
    return inflight;
  }

  function schedule() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      void writeNow();
    }, debounceMs);
  }

  return {
    load() {
      if (!loading) loading = doLoad();
      return loading;
    },
    get loaded() {
      return loaded;
    },
    get<T>(key: string) {
      return doc[key] as T | undefined;
    },
    snapshot() {
      return doc;
    },
    patch(key, value) {
      if (value === undefined) {
        if (!(key in doc)) return;
        delete doc[key];
      } else {
        doc[key] = value;
      }
      dirty = true;
      schedule();
    },
    async flush() {
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      if (dirty || inflight) await writeNow();
    },
  };
}
