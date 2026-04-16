import * as ipc from "../../ipc";
import type { SysInfoSnapshot } from "../../ipc";

let refreshTimer: ReturnType<typeof setInterval> | null = null;

export function showSysinfoDialog() {
  document.querySelector(".dialog-backdrop")?.remove();
  if (refreshTimer) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "820px";
  dialog.style.maxHeight = "85vh";

  dialog.innerHTML = `
    <div class="dialog-header">
      <span class="dialog-title">System Info</span>
      <button class="dialog-close">&times;</button>
    </div>
    <div class="sysinfo-body">
      <div class="sysinfo-loading">Loading system info&hellip;</div>
    </div>
  `;

  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const body = dialog.querySelector<HTMLElement>(".sysinfo-body")!;
  loadSysinfo(body);
  refreshTimer = setInterval(() => loadSysinfo(body), 3000);

  function close() {
    if (refreshTimer) {
      clearInterval(refreshTimer);
      refreshTimer = null;
    }
    backdrop.remove();
  }

  dialog.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) close(); });
  backdrop.addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
  backdrop.setAttribute("tabindex", "0");
  backdrop.focus();
}

async function loadSysinfo(container: HTMLElement) {
  try {
    const snap = await ipc.getSysinfoDetailed();
    renderSysinfo(container, snap);
  } catch (err) {
    container.innerHTML = `<div class="sysinfo-error">Failed to load system info</div>`;
    console.error("sysinfo error:", err);
  }
}

function renderSysinfo(container: HTMLElement, s: SysInfoSnapshot) {
  const memPct = s.mem_total_gb > 0 ? (s.mem_used_gb / s.mem_total_gb) * 100 : 0;

  let html = `
    <div class="sysinfo-header">
      <span class="sysinfo-hostname">${esc(s.hostname || "localhost")}</span>
      <span class="sysinfo-os">${esc(s.os_name)}</span>
      ${s.uptime_secs != null ? `<span class="sysinfo-uptime">up ${formatUptime(s.uptime_secs)}</span>` : ""}
      <span class="sysinfo-time">${esc(s.timestamp)}</span>
    </div>
  `;

  // Summary gauges: CPU avg, RAM, Disk /, Battery
  html += `<div class="sysinfo-gauges">`;
  html += gaugeCard("CPU", s.cpu_percent, `${s.cpu_percent.toFixed(0)}% avg (${s.cpu_cores.length} threads)`);
  html += gaugeCard("RAM", memPct, `${s.mem_used_gb.toFixed(1)} / ${s.mem_total_gb.toFixed(1)} GB`);

  if (s.disk) {
    const dPct = s.disk.total_gb > 0 ? (s.disk.used_gb / s.disk.total_gb) * 100 : 0;
    html += gaugeCard("Disk /", dPct, `${s.disk.used_gb.toFixed(1)} / ${s.disk.total_gb.toFixed(0)} GB`);
  }

  if (s.battery_percent != null) {
    const charge = s.battery_charging ? " +" : "";
    html += gaugeCard("Battery", s.battery_percent, `${s.battery_percent.toFixed(0)}%${charge}`);
  }
  html += `</div>`;

  // Per-core CPU breakdown
  if (s.cpu_cores.length > 0) {
    html += `<div class="sysinfo-section-label">CPU Threads</div>`;
    html += `<div class="sysinfo-cores">`;
    for (const c of s.cpu_cores) {
      const pct = Math.max(0, Math.min(100, c.percent));
      const colorClass = pct >= 90 ? "gauge-critical" : pct >= 70 ? "gauge-warn" : "gauge-ok";
      html += `
        <div class="sysinfo-core">
          <span class="sysinfo-core-id">${c.core}</span>
          <div class="sysinfo-core-track">
            <div class="sysinfo-core-fill ${colorClass}" style="width:${pct}%"></div>
          </div>
          <span class="sysinfo-core-pct">${pct.toFixed(0)}%</span>
        </div>
      `;
    }
    html += `</div>`;
  }

  // Load average
  if (s.load_avg) {
    html += `
      <div class="sysinfo-load">
        <span class="sysinfo-load-label">Load avg</span>
        <span class="sysinfo-load-val">${s.load_avg[0].toFixed(2)}</span>
        <span class="sysinfo-load-val">${s.load_avg[1].toFixed(2)}</span>
        <span class="sysinfo-load-val">${s.load_avg[2].toFixed(2)}</span>
      </div>
    `;
  }

  container.innerHTML = html;
}

function gaugeCard(label: string, percent: number, detail: string): string {
  const clamped = Math.max(0, Math.min(100, percent));
  const colorClass = clamped >= 90 ? "gauge-critical" : clamped >= 70 ? "gauge-warn" : "gauge-ok";
  return `
    <div class="sysinfo-gauge">
      <div class="sysinfo-gauge-header">
        <span class="sysinfo-gauge-label">${esc(label)}</span>
        <span class="sysinfo-gauge-pct">${clamped.toFixed(0)}%</span>
      </div>
      <div class="sysinfo-gauge-track">
        <div class="sysinfo-gauge-fill ${colorClass}" style="width:${clamped}%"></div>
      </div>
      <div class="sysinfo-gauge-detail">${esc(detail)}</div>
    </div>
  `;
}

function formatUptime(secs: number): string {
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  const parts: string[] = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  parts.push(`${mins}m`);
  return parts.join(" ");
}

function esc(t: string): string {
  return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
