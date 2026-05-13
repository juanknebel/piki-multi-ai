// Pure helpers for the WebPreview tab. No DOM, no state.

export interface PortPreset {
  port: number;
  label: string;
}

export const PORT_PRESETS: PortPreset[] = [
  { port: 5173,  label: "Vite (5173)" },
  { port: 3000,  label: "Next.js / Node (3000)" },
  { port: 3001,  label: "Next.js alt (3001)" },
  { port: 8080,  label: "Webpack / Generic (8080)" },
  { port: 8000,  label: "Django / Python (8000)" },
  { port: 4173,  label: "Vite preview (4173)" },
  { port: 4200,  label: "Angular (4200)" },
  { port: 4000,  label: "Phoenix / Generic (4000)" },
  { port: 5000,  label: "Flask / Generic (5000)" },
  { port: 9000,  label: "PHP / Generic (9000)" },
  { port: 1313,  label: "Hugo (1313)" },
  { port: 11434, label: "Ollama (11434)" },
  { port: 1234,  label: "Parcel / llama.cpp (1234)" },
  { port: 8888,  label: "Jupyter (8888)" },
  { port: 6006,  label: "Storybook (6006)" },
  { port: 3030,  label: "Generic dev (3030)" },
];

/** Turn user-typed text into a full URL.
 *  Anything that looks like a localhost/127.0.0.1/[::1]/*.localhost host gets
 *  `http://`; everything else gets `https://`. Pre-existing schemes are kept. */
export function normalizeUrl(input: string): string {
  const s = input.trim();
  if (!s) return "";
  if (/^https?:\/\//i.test(s)) return s;
  if (/^(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$|\?|#)/.test(s)) return `http://${s}`;
  if (/^[\w-]+\.localhost(:\d+)?(\/|$|\?|#)/.test(s)) return `http://${s}`;
  return `https://${s}`;
}

export function isLocalUrl(url: string): boolean {
  try {
    const h = new URL(url).hostname;
    return h === "localhost" || h === "127.0.0.1" || h === "[::1]"
        || h.endsWith(".localhost");
  } catch {
    return false;
  }
}

/** Resolves if the URL responds within `timeoutMs`, rejects otherwise.
 *  Uses `no-cors` so we only care whether the request completes — the response
 *  body and status are opaque. False positives are possible if something else
 *  is bound to the port (acceptable for v1). */
export async function probeUrl(url: string, timeoutMs = 900, externalSignal?: AbortSignal): Promise<void> {
  const ctrl = new AbortController();
  const onAbort = () => ctrl.abort();
  externalSignal?.addEventListener("abort", onAbort);
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    await fetch(url, {
      method: "GET",
      mode: "no-cors",
      cache: "no-store",
      signal: ctrl.signal,
    });
  } finally {
    clearTimeout(timer);
    externalSignal?.removeEventListener("abort", onAbort);
  }
}
