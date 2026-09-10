/** Pure path → kind rules shared by the file finder and the tree: which
 *  files get an editor tab and which are not text at all. No DOM. */

const MD_RE = /\.(md|markdown)$/i;

/** Extensions we never try to load into CodeMirror: images (except SVG,
 *  which is XML), fonts, archives, media, compiled and database blobs. The
 *  backend reads files as UTF-8, so these would only surface as an error. */
const BINARY_EXT = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tiff", "tif", "heic", "psd",
  "woff", "woff2", "ttf", "otf", "eot",
  "zip", "tar", "gz", "tgz", "bz2", "xz", "zst", "7z", "rar", "jar", "iso", "dmg",
  "pdf",
  "mp3", "mp4", "m4a", "mov", "avi", "mkv", "wav", "ogg", "flac", "webm",
  "exe", "dll", "so", "dylib", "o", "a", "lib", "obj", "wasm", "class", "pyc", "pyo",
  "db", "sqlite", "sqlite3", "bin", "dat",
]);

/** `.md` / `.markdown` (case-insensitive) — opens in the markdown editor. */
export function isMarkdownPath(path: string): boolean {
  return MD_RE.test(path);
}

/** True when the extension says the file is not text; such files stay in
 *  the read-only viewer instead of getting an editor tab. */
export function looksBinary(path: string): boolean {
  const name = path.split("/").pop() ?? path;
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return false;
  return BINARY_EXT.has(name.slice(dot + 1).toLowerCase());
}
