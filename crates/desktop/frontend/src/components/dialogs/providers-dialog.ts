import { listProviders, saveProvider, deleteProvider, ProviderDetail } from "../../ipc";
import { toast } from "../toast";
import { invalidateProviderCache } from "../menu-bar";
import { createDropdown, type DropdownHandle } from "../dropdown";

export async function showProvidersDialog() {
  document.querySelector(".providers-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop providers-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog";
  dialog.style.maxWidth = "700px";
  dialog.style.maxHeight = "80vh";

  const header = document.createElement("div");
  header.className = "dialog-header";
  header.innerHTML = `
    <span class="dialog-title">Manage Providers</span>
    <button class="dialog-close">&times;</button>
  `;

  const body = document.createElement("div");
  body.className = "dialog-body";
  body.style.overflowY = "auto";
  body.style.padding = "12px";

  dialog.appendChild(header);
  dialog.appendChild(body);
  backdrop.appendChild(dialog);
  document.body.appendChild(backdrop);

  const close = () => {
    backdrop.remove();
    document.removeEventListener("keydown", onKey);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };
  document.addEventListener("keydown", onKey);
  header.querySelector(".dialog-close")!.addEventListener("click", close);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) close();
  });

  async function render() {
    const providers = await listProviders();

    body.innerHTML = "";

    // Provider list
    if (providers.length === 0) {
      body.innerHTML = `<div style="color:var(--text-muted);padding:12px">No providers configured.</div>`;
    } else {
      const table = document.createElement("table");
      table.style.width = "100%";
      table.style.borderCollapse = "collapse";
      table.innerHTML = `
        <thead>
          <tr style="text-align:left;color:var(--text-muted);font-size:12px;border-bottom:1px solid var(--border)">
            <th style="padding:4px 8px">Name</th>
            <th style="padding:4px 8px">Command</th>
            <th style="padding:4px 8px">Format</th>
            <th style="padding:4px 8px">Dispatch</th>
            <th style="padding:4px 8px"></th>
          </tr>
        </thead>
      `;
      const tbody = document.createElement("tbody");
      for (const p of providers) {
        const tr = document.createElement("tr");
        tr.style.borderBottom = "1px solid var(--border)";
        tr.innerHTML = `
          <td style="padding:6px 8px">${esc(p.name)}</td>
          <td style="padding:6px 8px;color:var(--text-muted)">${esc(p.command)}</td>
          <td style="padding:6px 8px;color:var(--text-muted)">${esc(p.prompt_format)}</td>
          <td style="padding:6px 8px">${p.dispatchable ? "Yes" : "No"}</td>
          <td style="padding:6px 8px;text-align:right">
            <button class="btn-edit" data-name="${escAttr(p.name)}" style="margin-right:4px">Edit</button>
            <button class="btn-delete" data-name="${escAttr(p.name)}">Delete</button>
          </td>
        `;
        tbody.appendChild(tr);
      }
      table.appendChild(tbody);
      body.appendChild(table);
    }

    // Action buttons
    const actions = document.createElement("div");
    actions.style.marginTop = "12px";
    actions.style.display = "flex";
    actions.style.gap = "8px";
    actions.innerHTML = `<button class="btn-new" style="padding:6px 16px">New Provider</button>`;
    body.appendChild(actions);

    // Wire events
    body.querySelectorAll<HTMLButtonElement>(".btn-edit").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const name = btn.dataset.name!;
        const prov = providers.find((p) => p.name === name);
        if (prov) await showEditForm(prov);
      });
    });

    body.querySelectorAll<HTMLButtonElement>(".btn-delete").forEach((btn) => {
      btn.addEventListener("click", () => {
        const name = btn.dataset.name!;
        showDeleteConfirm(name);
      });
    });

    body.querySelector<HTMLButtonElement>(".btn-new")!.addEventListener("click", async () => {
      await showEditForm(null);
    });
  }

  function showDeleteConfirm(name: string) {
    body.innerHTML = `
      <div style="display:flex;flex-direction:column;align-items:center;gap:16px;padding:24px 12px">
        <span style="font-size:13px;color:var(--text-primary)">Delete provider <strong>${esc(name)}</strong>?</span>
        <div style="display:flex;gap:8px">
          <button class="confirm-cancel" style="padding:6px 20px">Cancel</button>
          <button class="confirm-ok" style="padding:6px 20px;background:var(--status-error,#bf616a);color:#fff;border-color:transparent">Delete</button>
        </div>
      </div>
    `;
    body.querySelector(".confirm-cancel")!.addEventListener("click", () => render());
    body.querySelector(".confirm-ok")!.addEventListener("click", async () => {
      await deleteProvider(name);
      invalidateProviderCache();
      toast(`Provider deleted: ${name}`, "success");
      await render();
    });
  }

  async function showEditForm(existing: ProviderDetail | null) {
    const isEdit = existing !== null;
    const p: ProviderDetail = existing ?? {
      name: "",
      description: "",
      command: "",
      default_args: [],
      prompt_format: "Positional",
      prompt_flag: "",
      dispatchable: true,
      agent_dir: null,
    };

    body.innerHTML = `
      <div style="display:flex;flex-direction:column;gap:8px">
        <div class="form-title" style="font-weight:bold;margin-bottom:4px">${isEdit ? "Edit" : "New"} Provider</div>
        <label style="display:flex;flex-direction:column;gap:2px">
          <span style="color:var(--text-muted);font-size:12px">Name</span>
          <input class="f-name" type="text" value="${escAttr(p.name)}" ${isEdit ? "readonly" : ""} />
        </label>
        <label style="display:flex;flex-direction:column;gap:2px">
          <span style="color:var(--text-muted);font-size:12px">Description</span>
          <input class="f-desc" type="text" value="${escAttr(p.description)}" />
        </label>
        <label style="display:flex;flex-direction:column;gap:2px">
          <span style="color:var(--text-muted);font-size:12px">Command (binary path or name)</span>
          <input class="f-cmd" type="text" value="${escAttr(p.command)}" />
        </label>
        <label style="display:flex;flex-direction:column;gap:2px">
          <span style="color:var(--text-muted);font-size:12px">Default Args (space-separated)</span>
          <input class="f-args" type="text" value="${escAttr(p.default_args.join(" "))}" />
        </label>
        <div style="display:flex;flex-direction:column;gap:2px">
          <span style="color:var(--text-muted);font-size:12px">Prompt Format</span>
          <div class="f-format-slot"></div>
        </div>
        <label class="flag-row" style="display:flex;flex-direction:column;gap:2px;${p.prompt_format !== "Flag" ? "display:none" : ""}">
          <span style="color:var(--text-muted);font-size:12px">Flag (e.g. --prompt)</span>
          <input class="f-flag" type="text" value="${escAttr(p.prompt_flag)}" />
        </label>
        <label style="display:flex;align-items:center;gap:8px">
          <input class="f-dispatch" type="checkbox" ${p.dispatchable ? "checked" : ""} />
          <span style="color:var(--text-muted);font-size:12px">Dispatchable (available as agent provider)</span>
        </label>
        <label style="display:flex;flex-direction:column;gap:2px">
          <span style="color:var(--text-muted);font-size:12px">Agent Dir (e.g. .my-ai/agents)</span>
          <input class="f-agentdir" type="text" value="${escAttr(p.agent_dir ?? "")}" />
        </label>
        <div style="display:flex;gap:8px;margin-top:8px">
          <button class="btn-save" style="padding:6px 16px">Save</button>
          <button class="btn-cancel" style="padding:6px 16px">Cancel</button>
        </div>
      </div>
    `;

    // Custom dropdown for prompt format (native <select> ignores dark theme styling)
    const formatDropdown: DropdownHandle = createDropdown(
      [
        { value: "Positional", label: "Positional" },
        { value: "Flag", label: "Flag" },
        { value: "None", label: "None" },
      ],
      p.prompt_format,
    );
    body.querySelector<HTMLElement>(".f-format-slot")!.appendChild(formatDropdown.container);

    const flagRow = body.querySelector<HTMLElement>(".flag-row")!;
    formatDropdown.container.addEventListener("change", () => {
      flagRow.style.display = formatDropdown.value === "Flag" ? "flex" : "none";
    });

    body.querySelector(".btn-cancel")!.addEventListener("click", () => render());

    body.querySelector(".btn-save")!.addEventListener("click", async () => {
      const name = (body.querySelector<HTMLInputElement>(".f-name")!).value.trim();
      const command = (body.querySelector<HTMLInputElement>(".f-cmd")!).value.trim();
      if (!name || !command) {
        toast("Name and command are required", "error");
        return;
      }
      const argsStr = (body.querySelector<HTMLInputElement>(".f-args")!).value.trim();
      const detail: ProviderDetail = {
        name,
        description: (body.querySelector<HTMLInputElement>(".f-desc")!).value.trim(),
        command,
        default_args: argsStr ? argsStr.split(/\s+/) : [],
        prompt_format: formatDropdown.value,
        prompt_flag: (body.querySelector<HTMLInputElement>(".f-flag")!).value.trim(),
        dispatchable: (body.querySelector<HTMLInputElement>(".f-dispatch")!).checked,
        agent_dir: (body.querySelector<HTMLInputElement>(".f-agentdir")!).value.trim() || null,
      };
      try {
        await saveProvider(detail);
        invalidateProviderCache();
        toast(`Provider saved: ${name}`, "success");
        await render();
      } catch (err) {
        toast(`Failed to save: ${err}`, "error");
      }
    });
  }

  await render();
}

function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function escAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
