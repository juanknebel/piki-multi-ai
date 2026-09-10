import { describe, expect, it } from "vitest";
import { isMarkdownPath, looksBinary } from "./file-kind";

describe("looksBinary", () => {
  it("flags images, archives, fonts, media and compiled blobs by extension", () => {
    for (const p of ["logo.PNG", "docs/a.pdf", "dist/app.wasm", "x/y.tar.gz", "font.woff2", "clip.mp4", "data.sqlite"]) {
      expect(looksBinary(p), p).toBe(true);
    }
  });

  it("keeps source, config, svg and extension-less files as text", () => {
    for (const p of ["src/main.rs", "README", ".gitignore", "icon.svg", "Makefile", "a.b/c", "notes.txt"]) {
      expect(looksBinary(p), p).toBe(false);
    }
  });
});

describe("isMarkdownPath", () => {
  it("matches .md and .markdown regardless of case", () => {
    expect(isMarkdownPath("README.md")).toBe(true);
    expect(isMarkdownPath("docs/x.MARKDOWN")).toBe(true);
    expect(isMarkdownPath("md/main.rs")).toBe(false);
  });
});
