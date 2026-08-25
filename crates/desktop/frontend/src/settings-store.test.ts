import { describe, expect, it, vi } from "vitest";
import { createSettingsStore, type SettingsBackend } from "./settings-store";

function fakeBackend(initial: string | null = null) {
  const writes: string[] = [];
  let stored = initial;
  let failNext = false;
  const backend: SettingsBackend = {
    read: async () => stored,
    write: async (json) => {
      if (failNext) {
        failNext = false;
        throw new Error("disk full");
      }
      writes.push(json);
      stored = json;
    },
  };
  return { backend, writes, get stored() { return stored; }, failNextWrite: () => { failNext = true; } };
}

describe("settings-store", () => {
  it("loads the document once and serves get() from memory", async () => {
    const fb = fakeBackend(JSON.stringify({ sidebarWidth: 300, shell: "zsh" }));
    const readSpy = vi.spyOn(fb.backend, "read");
    const store = createSettingsStore(fb.backend);
    expect(store.loaded).toBe(false);
    expect(store.get("sidebarWidth")).toBeUndefined();
    await Promise.all([store.load(), store.load()]);
    expect(readSpy).toHaveBeenCalledTimes(1);
    expect(store.loaded).toBe(true);
    expect(store.get<number>("sidebarWidth")).toBe(300);
    expect(store.get("missing")).toBeUndefined();
  });

  it("coalesces rapid patches of different keys into one full-document write", async () => {
    // The bug this store exists for: a sidebar drag landing while the
    // pane-layout flush is pending used to clobber one of the two.
    const fb = fakeBackend(JSON.stringify({ shell: "fish" }));
    const store = createSettingsStore(fb.backend, { debounceMs: 5 });
    await store.load();
    store.patch("wsTabsV2", { a: 1 });
    store.patch("sidebarWidth", 240);
    await store.flush();
    expect(fb.writes).toHaveLength(1);
    expect(JSON.parse(fb.writes[0])).toEqual({ shell: "fish", wsTabsV2: { a: 1 }, sidebarWidth: 240 });
  });

  it("keeps keys other callers own when writing (never a partial document)", async () => {
    const fb = fakeBackend(JSON.stringify({ shell: "zsh", shortcuts: { help: "F1" } }));
    const store = createSettingsStore(fb.backend, { debounceMs: 1 });
    await store.load();
    store.patch("chatPanelWidth", 400);
    await store.flush();
    expect(JSON.parse(fb.stored!)).toEqual({ shell: "zsh", shortcuts: { help: "F1" }, chatPanelWidth: 400 });
  });

  it("serializes a patch that arrives mid-write into a follow-up write", async () => {
    const fb = fakeBackend("{}");
    let release!: () => void;
    const gate = new Promise<void>((r) => { release = r; });
    const slowWrite = vi.spyOn(fb.backend, "write").mockImplementationOnce(async (json) => {
      await gate;
      fb.writes.push(json);
    });
    const store = createSettingsStore(fb.backend, { debounceMs: 1 });
    await store.load();
    store.patch("a", 1);
    const first = store.flush();
    store.patch("b", 2); // lands while write #1 is blocked
    release();
    await first;
    await store.flush();
    expect(slowWrite).toHaveBeenCalled();
    expect(fb.writes.map((w) => JSON.parse(w))).toEqual([{ a: 1 }, { a: 1, b: 2 }]);
  });

  it("survives a failed write: the snapshot keeps the value and the next write carries it", async () => {
    const fb = fakeBackend("{}");
    const onError = vi.fn();
    const store = createSettingsStore(fb.backend, { debounceMs: 1, onError });
    await store.load();
    fb.failNextWrite();
    store.patch("sidebarWidth", 200);
    await store.flush();
    expect(onError).toHaveBeenCalledTimes(1);
    expect(fb.writes).toHaveLength(0);
    expect(store.get("sidebarWidth")).toBe(200);
    store.patch("agentsPanelHeight", 120);
    await store.flush();
    expect(JSON.parse(fb.stored!)).toEqual({ sidebarWidth: 200, agentsPanelHeight: 120 });
  });

  it("deletes a key when patched with undefined", async () => {
    const fb = fakeBackend(JSON.stringify({ shell: "zsh" }));
    const store = createSettingsStore(fb.backend, { debounceMs: 1 });
    await store.load();
    store.patch("shell", undefined);
    await store.flush();
    expect(JSON.parse(fb.stored!)).toEqual({});
    expect(store.get("shell")).toBeUndefined();
  });

  it("starts from an empty document when the stored one is corrupt", async () => {
    const fb = fakeBackend("{not json");
    const store = createSettingsStore(fb.backend, { debounceMs: 1 });
    await store.load();
    expect(store.snapshot()).toEqual({});
    store.patch("shell", "bash");
    await store.flush();
    expect(JSON.parse(fb.stored!)).toEqual({ shell: "bash" });
  });
});
