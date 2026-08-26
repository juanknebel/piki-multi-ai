import { describe, it, expect } from "vitest";
import { decodeBase64Bytes, decodeBase64BytesLegacy, decodePtyFrame, toBytes } from "./pty-frame";

function frame(tabId: string, data: number[]): Uint8Array {
  const id = new TextEncoder().encode(tabId);
  return Uint8Array.from([id.length, ...id, ...data]);
}

describe("decodePtyFrame", () => {
  it("splits the tab id from the payload", () => {
    const f = decodePtyFrame(frame("abc", [120, 121, 122]));
    expect(f?.tabId).toBe("abc");
    expect(Array.from(f!.data)).toEqual([120, 121, 122]);
  });

  it("handles an empty payload and a UUID id", () => {
    const id = "6f1c2d3e-4b5a-4c6d-8e9f-0a1b2c3d4e5f";
    const f = decodePtyFrame(frame(id, []));
    expect(f?.tabId).toBe(id);
    expect(f?.data.length).toBe(0);
  });

  it("rejects a frame shorter than its header", () => {
    expect(decodePtyFrame(new Uint8Array([]))).toBeNull();
    expect(decodePtyFrame(new Uint8Array([5, 97]))).toBeNull();
  });

  it("does not copy the payload", () => {
    const raw = frame("t", [1, 2, 3]);
    const f = decodePtyFrame(raw)!;
    expect(f.data.buffer).toBe(raw.buffer);
  });
});

describe("toBytes", () => {
  it("accepts ArrayBuffer, typed arrays, views and plain arrays", () => {
    const buf = new Uint8Array([9, 8]).buffer;
    expect(Array.from(toBytes(buf)!)).toEqual([9, 8]);
    expect(Array.from(toBytes(new Uint8Array([1]))!)).toEqual([1]);
    expect(Array.from(toBytes(new DataView(buf, 1))!)).toEqual([8]);
    expect(Array.from(toBytes([4, 5])!)).toEqual([4, 5]);
    expect(toBytes("nope")).toBeNull();
    expect(toBytes(null)).toBeNull();
  });
});

describe("decodeBase64Bytes", () => {
  it("matches the legacy decoder byte for byte", () => {
    const bytes = new Uint8Array(4096);
    for (let i = 0; i < bytes.length; i++) bytes[i] = (i * 31) & 0xff;
    let bin = "";
    for (const b of bytes) bin += String.fromCharCode(b);
    const b64 = btoa(bin);
    expect(Array.from(decodeBase64Bytes(b64))).toEqual(Array.from(decodeBase64BytesLegacy(b64)));
    expect(Array.from(decodeBase64Bytes(b64))).toEqual(Array.from(bytes));
  });

  // Reproducible decode benchmark for docs/performance.md:
  //   cd crates/desktop/frontend && PIKI_BENCH=1 npx vitest run src/pty-frame.test.ts
  // Always passes; prints ms per 8 MB of PTY output for each decode path.
  it("benchmark: legacy vs indexed base64 vs raw frame", () => {
    const env = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env;
    if (!env?.PIKI_BENCH) return;
    const CHUNK = 64 * 1024;
    const CHUNKS = 128; // 8 MB
    const chunk = new Uint8Array(CHUNK);
    for (let i = 0; i < CHUNK; i++) chunk[i] = 32 + (i % 90);
    let bin = "";
    for (const b of chunk) bin += String.fromCharCode(b);
    const b64 = btoa(bin);
    const raw = frame("6f1c2d3e-4b5a-4c6d-8e9f-0a1b2c3d4e5f", Array.from(chunk));
    const time = (label: string, fn: () => void) => {
      fn(); // warm-up
      const t = performance.now();
      for (let i = 0; i < CHUNKS; i++) fn();
      const ms = performance.now() - t;
      console.log(`${label}: ${ms.toFixed(1)} ms per ${(CHUNK * CHUNKS) / (1024 * 1024)} MB`);
    };
    time("base64 legacy (Uint8Array.from + callback)", () => decodeBase64BytesLegacy(b64));
    time("base64 indexed loop", () => decodeBase64Bytes(b64));
    time("raw channel frame (decodePtyFrame)", () => decodePtyFrame(raw));
  });
});
