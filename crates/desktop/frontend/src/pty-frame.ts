// Wire format of the raw PTY output channel (pure, vitest-covered).
//
// The backend (`crates/desktop/src/pty_output.rs::encode_frame`) ships one
// binary message per batch: `len(tabId) as u8`, the tab id bytes, then the
// PTY bytes. No JSON, no base64 — the bytes land in xterm as-is.

export interface PtyFrame {
  tabId: string;
  data: Uint8Array;
}

const asciiDecoder = new TextDecoder("utf-8");

/** Split one raw-channel message into its tab id and payload. Returns null
 *  for a malformed frame (too short for its own header). */
export function decodePtyFrame(bytes: Uint8Array): PtyFrame | null {
  if (bytes.length < 1) return null;
  const idLen = bytes[0];
  if (bytes.length < 1 + idLen) return null;
  const tabId = asciiDecoder.decode(bytes.subarray(1, 1 + idLen));
  return { tabId, data: bytes.subarray(1 + idLen) };
}

/** Whatever the Tauri channel hands us — `ArrayBuffer` on both its eval and
 *  fetch paths, but be lenient — as a byte view without copying. */
export function toBytes(message: unknown): Uint8Array | null {
  if (message instanceof ArrayBuffer) return new Uint8Array(message);
  if (message instanceof Uint8Array) return message;
  if (ArrayBuffer.isView(message)) {
    return new Uint8Array(message.buffer, message.byteOffset, message.byteLength);
  }
  if (Array.isArray(message)) return Uint8Array.from(message as number[]);
  return null;
}

/** Base64 → bytes for the JSON `pty-output` event fallback. An indexed loop
 *  over the binary string — `Uint8Array.from(str, cb)` pays a callback per
 *  byte. */
export function decodeBase64Bytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const n = bin.length;
  const out = new Uint8Array(n);
  for (let i = 0; i < n; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** The old decoder, kept only for the benchmark in `pty-frame.test.ts`. */
export function decodeBase64BytesLegacy(b64: string): Uint8Array {
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
}
