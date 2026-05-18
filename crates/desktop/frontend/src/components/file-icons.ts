// Per-file-type icons rendered with Nerd Font glyphs (the global
// "JetBrainsMono NF Mono" face). Codepoints are the authoritative values from
// the nerd-fonts `glyphnames.json` set, written as \u escapes so the source
// is unambiguous regardless of editor/terminal font.

export interface FileIcon {
  glyph: string;
  /** Always "fi" + one color-bucket modifier, e.g. "fi fi-rust". */
  cls: string;
}

interface GlyphDef {
  glyph: string;
  bucket: string;
}

const G = {
  rust: "", // dev-rust
  ts: "", // seti-typescript
  react: "", // seti-react
  js: "", // seti-javascript
  py: "", // seti-python
  go: "", // seti-go
  c: "", // seti-c
  cpp: "", // seti-cpp
  java: "", // dev-java
  ruby: "", // dev-ruby
  php: "", // seti-php
  shell: "", // seti-shell
  html: "", // dev-html5
  css: "", // seti-css
  sass: "", // seti-sass
  vue: "", // seti-vue
  svelte: "", // seti-svelte
  json: "", // seti-json
  config: "", // seti-config
  yml: "", // seti-yml
  xml: "", // seti-xml
  csv: "", // seti-csv
  db: "", // seti-db
  database: "", // fa-database
  markdown: "", // seti-markdown
  text: "", // seti-text / seti-default
  pdf: "", // seti-pdf
  image: "", // seti-image
  zip: "", // seti-zip
  lock: "", // fa-lock
  gear: "", // fa-gear
  npm: "", // seti-npm
  tsconfig: "", // seti-tsconfig
  docker: "", // seti-docker
  makefile: "", // seti-makefile
  git: "", // seti-git
  license: "", // fa-balance_scale
  file: "", // seti-default
  folder: "", // fa-folder
  folderOpen: "", // fa-folder_open
} as const;

const DEFAULT_FILE: GlyphDef = { glyph: G.file, bucket: "default" };
const FOLDER_CLOSED: GlyphDef = { glyph: G.folder, bucket: "folder" };
const FOLDER_OPEN: GlyphDef = { glyph: G.folderOpen, bucket: "folder" };

// Exact filename (lowercased) → icon. Highest priority.
const SPECIAL_FILES: Record<string, GlyphDef> = {
  "package.json": { glyph: G.npm, bucket: "web" },
  "package-lock.json": { glyph: G.npm, bucket: "muted" },
  "yarn.lock": { glyph: G.npm, bucket: "muted" },
  "pnpm-lock.yaml": { glyph: G.npm, bucket: "muted" },
  "tsconfig.json": { glyph: G.tsconfig, bucket: "ts" },
  "cargo.toml": { glyph: G.rust, bucket: "rust" },
  "cargo.lock": { glyph: G.lock, bucket: "muted" },
  dockerfile: { glyph: G.docker, bucket: "data" },
  ".dockerignore": { glyph: G.docker, bucket: "muted" },
  "docker-compose.yml": { glyph: G.docker, bucket: "data" },
  "docker-compose.yaml": { glyph: G.docker, bucket: "data" },
  "compose.yaml": { glyph: G.docker, bucket: "data" },
  makefile: { glyph: G.makefile, bucket: "default" },
  ".gitignore": { glyph: G.git, bucket: "muted" },
  ".gitattributes": { glyph: G.git, bucket: "muted" },
  ".gitmodules": { glyph: G.git, bucket: "muted" },
  ".gitkeep": { glyph: G.git, bucket: "muted" },
  "readme.md": { glyph: G.markdown, bucket: "doc" },
  readme: { glyph: G.markdown, bucket: "doc" },
  license: { glyph: G.license, bucket: "doc" },
  "license.md": { glyph: G.license, bucket: "doc" },
  "license.txt": { glyph: G.license, bucket: "doc" },
  ".env": { glyph: G.gear, bucket: "muted" },
  ".env.local": { glyph: G.gear, bucket: "muted" },
  ".env.production": { glyph: G.gear, bucket: "muted" },
  ".env.development": { glyph: G.gear, bucket: "muted" },
  ".editorconfig": { glyph: G.config, bucket: "muted" },
  ".npmrc": { glyph: G.config, bucket: "muted" },
  ".nvmrc": { glyph: G.config, bucket: "muted" },
  ".prettierrc": { glyph: G.config, bucket: "muted" },
  ".eslintrc": { glyph: G.config, bucket: "muted" },
  ".eslintrc.json": { glyph: G.config, bucket: "muted" },
  ".eslintrc.js": { glyph: G.config, bucket: "muted" },
};

// Compound suffixes, longest-first; first endsWith wins.
const COMPOUND_EXT: Array<[string, GlyphDef]> = [
  [".config.ts", { glyph: G.config, bucket: "data" }],
  [".config.js", { glyph: G.config, bucket: "data" }],
  [".config.mjs", { glyph: G.config, bucket: "data" }],
  [".config.cjs", { glyph: G.config, bucket: "data" }],
  [".d.ts", { glyph: G.ts, bucket: "ts" }],
];

// Simple extension (lowercased, no dot) → icon.
const EXT_MAP: Record<string, GlyphDef> = {
  rs: { glyph: G.rust, bucket: "rust" },
  ts: { glyph: G.ts, bucket: "ts" },
  tsx: { glyph: G.react, bucket: "ts" },
  mts: { glyph: G.ts, bucket: "ts" },
  cts: { glyph: G.ts, bucket: "ts" },
  js: { glyph: G.js, bucket: "js" },
  jsx: { glyph: G.react, bucket: "js" },
  mjs: { glyph: G.js, bucket: "js" },
  cjs: { glyph: G.js, bucket: "js" },
  py: { glyph: G.py, bucket: "py" },
  pyi: { glyph: G.py, bucket: "py" },
  go: { glyph: G.go, bucket: "go" },
  c: { glyph: G.c, bucket: "default" },
  h: { glyph: G.c, bucket: "default" },
  cpp: { glyph: G.cpp, bucket: "default" },
  cc: { glyph: G.cpp, bucket: "default" },
  cxx: { glyph: G.cpp, bucket: "default" },
  hpp: { glyph: G.cpp, bucket: "default" },
  hh: { glyph: G.cpp, bucket: "default" },
  java: { glyph: G.java, bucket: "default" },
  rb: { glyph: G.ruby, bucket: "default" },
  php: { glyph: G.php, bucket: "default" },
  sh: { glyph: G.shell, bucket: "default" },
  bash: { glyph: G.shell, bucket: "default" },
  zsh: { glyph: G.shell, bucket: "default" },
  fish: { glyph: G.shell, bucket: "default" },
  html: { glyph: G.html, bucket: "web" },
  htm: { glyph: G.html, bucket: "web" },
  css: { glyph: G.css, bucket: "web" },
  scss: { glyph: G.sass, bucket: "web" },
  sass: { glyph: G.sass, bucket: "web" },
  vue: { glyph: G.vue, bucket: "web" },
  svelte: { glyph: G.svelte, bucket: "web" },
  json: { glyph: G.json, bucket: "data" },
  jsonc: { glyph: G.json, bucket: "data" },
  toml: { glyph: G.config, bucket: "data" },
  ini: { glyph: G.config, bucket: "data" },
  cfg: { glyph: G.config, bucket: "data" },
  conf: { glyph: G.config, bucket: "data" },
  yaml: { glyph: G.yml, bucket: "data" },
  yml: { glyph: G.yml, bucket: "data" },
  xml: { glyph: G.xml, bucket: "data" },
  csv: { glyph: G.csv, bucket: "data" },
  sql: { glyph: G.db, bucket: "data" },
  db: { glyph: G.database, bucket: "data" },
  sqlite: { glyph: G.database, bucket: "data" },
  sqlite3: { glyph: G.database, bucket: "data" },
  md: { glyph: G.markdown, bucket: "doc" },
  markdown: { glyph: G.markdown, bucket: "doc" },
  txt: { glyph: G.text, bucket: "doc" },
  rst: { glyph: G.text, bucket: "doc" },
  pdf: { glyph: G.pdf, bucket: "doc" },
  log: { glyph: G.text, bucket: "muted" },
  png: { glyph: G.image, bucket: "asset" },
  jpg: { glyph: G.image, bucket: "asset" },
  jpeg: { glyph: G.image, bucket: "asset" },
  gif: { glyph: G.image, bucket: "asset" },
  webp: { glyph: G.image, bucket: "asset" },
  bmp: { glyph: G.image, bucket: "asset" },
  svg: { glyph: G.image, bucket: "asset" },
  ico: { glyph: G.image, bucket: "asset" },
  zip: { glyph: G.zip, bucket: "asset" },
  tar: { glyph: G.zip, bucket: "asset" },
  gz: { glyph: G.zip, bucket: "asset" },
  tgz: { glyph: G.zip, bucket: "asset" },
  bz2: { glyph: G.zip, bucket: "asset" },
  xz: { glyph: G.zip, bucket: "asset" },
  "7z": { glyph: G.zip, bucket: "asset" },
  rar: { glyph: G.zip, bucket: "asset" },
  lock: { glyph: G.lock, bucket: "muted" },
  env: { glyph: G.gear, bucket: "muted" },
};

function toIcon(def: GlyphDef): FileIcon {
  return { glyph: def.glyph, cls: `fi fi-${def.bucket}` };
}

export function fileGlyph(name: string): FileIcon {
  const lc = name.toLowerCase();
  const special = SPECIAL_FILES[lc];
  if (special) return toIcon(special);
  for (const [suffix, def] of COMPOUND_EXT) {
    if (lc.endsWith(suffix)) return toIcon(def);
  }
  const dot = lc.lastIndexOf(".");
  if (dot > 0) {
    const ext = EXT_MAP[lc.slice(dot + 1)];
    if (ext) return toIcon(ext);
  }
  return toIcon(DEFAULT_FILE);
}

export function folderGlyph(_name: string, open: boolean): FileIcon {
  return toIcon(open ? FOLDER_OPEN : FOLDER_CLOSED);
}
