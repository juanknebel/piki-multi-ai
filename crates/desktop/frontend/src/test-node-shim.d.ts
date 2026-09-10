// Minimal ambient types for the node built-ins the vitest suites use
// (src/css-invariants.test.ts reads the stylesheets from disk). The
// frontend deliberately has no @types/node; add signatures here as tests
// need them rather than pulling the whole package in.
declare module "node:fs" {
  export function readFileSync(path: string, encoding: "utf8"): string;
  export function readdirSync(path: string): string[];
  export function statSync(path: string): { isDirectory(): boolean };
}
