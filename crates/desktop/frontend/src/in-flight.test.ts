import { describe, expect, it } from "vitest";
import { isInFlight, onInFlightChange, runExclusive } from "./in-flight";

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("runExclusive", () => {
  it("a second call while the first is pending is a no-op (double click = one push)", async () => {
    const gate = deferred<string>();
    let calls = 0;
    const first = runExclusive("push", () => {
      calls++;
      return gate.promise;
    });
    const second = runExclusive("push", async () => {
      calls++;
      return "second";
    });
    expect(isInFlight("push")).toBe(true);
    expect(await second).toBeUndefined();
    gate.resolve("first");
    expect(await first).toBe("first");
    expect(calls).toBe(1);
    expect(isInFlight("push")).toBe(false);
  });

  it("runs again once the previous call settled", async () => {
    expect(await runExclusive("pull", async () => 1)).toBe(1);
    expect(await runExclusive("pull", async () => 2)).toBe(2);
  });

  it("keys are independent", async () => {
    const gate = deferred<void>();
    const push = runExclusive("push:0", () => gate.promise);
    expect(await runExclusive("push:1", async () => "other")).toBe("other");
    gate.resolve();
    await push;
  });

  it("a rejection releases the key and is rethrown", async () => {
    await expect(
      runExclusive("fail", async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");
    expect(isInFlight("fail")).toBe(false);
    expect(await runExclusive("fail", async () => "ok")).toBe("ok");
  });

  it("notifies busy then idle, and only for real runs", async () => {
    const seen: Array<[string, boolean]> = [];
    const off = onInFlightChange((key, busy) => seen.push([key, busy]));
    const gate = deferred<void>();
    const p = runExclusive("k", () => gate.promise);
    await runExclusive("k", async () => {}); // skipped: no notification
    gate.resolve();
    await p;
    off();
    await runExclusive("k", async () => {}); // after unsubscribe: silent
    expect(seen).toEqual([
      ["k", true],
      ["k", false],
    ]);
  });
});
