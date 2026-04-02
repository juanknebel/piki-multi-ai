import { getVersion } from "@tauri-apps/api/app";

export async function showAboutDialog() {
  const version = await getVersion().catch(() => "unknown");
  document.querySelector(".about-dialog-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop about-dialog-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "400px";
  dialog.style.textAlign = "center";

  dialog.innerHTML = `
    <div class="dialog-header">
      <span class="dialog-title">About</span>
      <button class="dialog-close">×</button>
    </div>
    <div class="dialog-body" style="align-items:center;padding:24px 20px">
      <div style="width:96px;height:96px;margin-bottom:12px">${LOGO_SVG}</div>
      <div style="font-size:18px;font-weight:700;color:var(--text-bright);letter-spacing:0.04em">Piki Desktop</div>
      <div style="font-size:11px;color:var(--text-secondary);letter-spacing:0.12em;text-transform:uppercase;margin-bottom:16px">Multi AI Workspace</div>
      <div style="font-size:12px;color:var(--text-primary);margin-bottom:4px">v${version}</div>
      <div style="width:100%;border-top:1px solid var(--border-primary);margin:12px 0"></div>
      <div style="font-size:12px;color:var(--text-secondary);line-height:1.8">
        <div>Author: <span style="color:var(--text-primary)">Juan Knebel</span></div>
        <div>Contact: <span style="color:var(--text-accent)">juanknebel@gmail.com</span></div>
        <div>Web: <span style="color:var(--text-accent)">github.com/juanknebel/piki-multi-ai</span></div>
        <div>License: <span style="color:var(--text-primary)">GPL-2.0</span></div>
      </div>
      <div style="width:100%;border-top:1px solid var(--border-primary);margin:12px 0"></div>
      <div style="font-size:10px;color:var(--text-muted);line-height:1.6">
        Built with Tauri v2, TypeScript, xterm.js<br>
        Powered by Rust + piki-core
      </div>
    </div>
    <div class="dialog-footer" style="justify-content:center">
      <button class="dialog-btn dialog-btn-secondary" id="about-close">Close</button>
    </div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const close = () => backdrop.remove();
  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  dialog.querySelector("#about-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

const LOGO_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="100%" height="100%">
  <defs>
    <linearGradient id="ab-bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#0b0f14"/>
      <stop offset="100%" stop-color="#151c25"/>
    </linearGradient>
    <linearGradient id="ab-glow" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="#39bae6"/>
      <stop offset="100%" stop-color="#7aa2f7"/>
    </linearGradient>
    <filter id="ab-blur"><feGaussianBlur in="SourceGraphic" stdDeviation="12"/></filter>
  </defs>
  <rect width="1024" height="1024" rx="180" fill="url(#ab-bg)"/>
  <rect x="4" y="4" width="1016" height="1016" rx="176" fill="none" stroke="#1a2231" stroke-width="8"/>
  <text x="512" y="600" font-family="monospace" font-size="620" font-weight="900"
        text-anchor="middle" fill="url(#ab-glow)" filter="url(#ab-blur)" opacity="0.35">K</text>
  <g transform="translate(248, 180)" fill="url(#ab-glow)">
    <rect x="0" y="0" width="52" height="52" rx="4"/><rect x="0" y="56" width="52" height="52" rx="4"/>
    <rect x="0" y="112" width="52" height="52" rx="4"/><rect x="0" y="168" width="52" height="52" rx="4"/>
    <rect x="0" y="224" width="52" height="52" rx="4"/><rect x="0" y="280" width="52" height="52" rx="4"/>
    <rect x="0" y="336" width="52" height="52" rx="4"/><rect x="0" y="392" width="52" height="52" rx="4"/>
    <rect x="0" y="448" width="52" height="52" rx="4"/><rect x="0" y="504" width="52" height="52" rx="4"/>
    <rect x="0" y="560" width="52" height="52" rx="4"/><rect x="0" y="616" width="52" height="52" rx="4"/>
    <rect x="56" y="0" width="52" height="52" rx="4"/><rect x="56" y="56" width="52" height="52" rx="4"/>
    <rect x="56" y="112" width="52" height="52" rx="4"/><rect x="56" y="168" width="52" height="52" rx="4"/>
    <rect x="56" y="224" width="52" height="52" rx="4"/><rect x="56" y="280" width="52" height="52" rx="4"/>
    <rect x="56" y="336" width="52" height="52" rx="4"/><rect x="56" y="392" width="52" height="52" rx="4"/>
    <rect x="56" y="448" width="52" height="52" rx="4"/><rect x="56" y="504" width="52" height="52" rx="4"/>
    <rect x="56" y="560" width="52" height="52" rx="4"/><rect x="56" y="616" width="52" height="52" rx="4"/>
    <rect x="336" y="0" width="52" height="52" rx="4"/><rect x="392" y="0" width="52" height="52" rx="4"/>
    <rect x="280" y="56" width="52" height="52" rx="4"/><rect x="336" y="56" width="52" height="52" rx="4"/>
    <rect x="224" y="112" width="52" height="52" rx="4"/><rect x="280" y="112" width="52" height="52" rx="4"/>
    <rect x="168" y="168" width="52" height="52" rx="4"/><rect x="224" y="168" width="52" height="52" rx="4"/>
    <rect x="112" y="224" width="52" height="52" rx="4"/><rect x="168" y="224" width="52" height="52" rx="4"/>
    <rect x="112" y="280" width="52" height="52" rx="4"/><rect x="168" y="280" width="52" height="52" rx="4"/>
    <rect x="112" y="336" width="52" height="52" rx="4"/><rect x="168" y="336" width="52" height="52" rx="4"/>
    <rect x="112" y="392" width="52" height="52" rx="4"/><rect x="168" y="392" width="52" height="52" rx="4"/>
    <rect x="168" y="448" width="52" height="52" rx="4"/><rect x="224" y="448" width="52" height="52" rx="4"/>
    <rect x="224" y="504" width="52" height="52" rx="4"/><rect x="280" y="504" width="52" height="52" rx="4"/>
    <rect x="280" y="560" width="52" height="52" rx="4"/><rect x="336" y="560" width="52" height="52" rx="4"/>
    <rect x="336" y="616" width="52" height="52" rx="4"/><rect x="392" y="616" width="52" height="52" rx="4"/>
    <rect x="448" y="0" width="52" height="52" rx="4"/><rect x="448" y="616" width="52" height="52" rx="4"/>
  </g>
  <rect x="120" y="900" width="784" height="3" rx="1.5" fill="url(#ab-glow)" opacity="0.4"/>
</svg>`;
