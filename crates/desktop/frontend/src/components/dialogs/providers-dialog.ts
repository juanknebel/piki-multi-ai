import { listProviders, saveProvider, deleteProvider, ProviderDetail } from "../../ipc";
import { toast } from "../toast";
import { invalidateProviderCache } from "../menu-bar";
import { createDropdown, type DropdownHandle } from "../dropdown";
import { attachPathPicker } from "../path-picker";

export async function showProvidersDialog() {
  document.querySelector(".providers-backdrop")?.remove();

  const backdrop = document.createElement("div");
  backdrop.className = "dialog-backdrop providers-backdrop";

  const dialog = document.createElement("div");
  dialog.className = "dialog providers-dialog";

  const header = document.createElement("div");
  header.className = "dialog-header";
  header.innerHTML = `
    <span class="dialog-title">Manage Providers</span>
    <button class="dialog-close">&times;</button>
  `;

  const body = document.createElement("div");
  body.className = "dialog-body";

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

    if (providers.length === 0) {
      const empty = document.createElement("div");
      empty.className = "providers-empty";
      empty.textContent = "No providers configured.";
      body.appendChild(empty);
    } else {
      const table = document.createElement("table");
      table.className = "providers-table";
      table.innerHTML = `
        <thead>
          <tr>
            <th>Name</th>
            <th>Command</th>
            <th>Format</th>
            <th>Dispatch</th>
            <th class="col-actions"></th>
          </tr>
        </thead>
      `;
      const tbody = document.createElement("tbody");
      for (const p of providers) {
        const tr = document.createElement("tr");
        tr.innerHTML = `
          <td>${esc(p.name)}</td>
          <td class="col-muted">${esc(p.command)}</td>
          <td class="col-muted">${esc(p.prompt_format)}</td>
          <td>
            <span class="dialog-badge ${p.dispatchable ? "success" : "muted"}">
              ${p.dispatchable ? "Yes" : "No"}
            </span>
          </td>
          <td class="col-actions">
            <button class="dialog-btn dialog-btn-secondary dialog-btn-sm btn-edit" data-name="${escAttr(p.name)}">Edit</button>
            <button class="dialog-btn dialog-btn-danger dialog-btn-sm btn-delete" data-name="${escAttr(p.name)}">Delete</button>
          </td>
        `;
        tbody.appendChild(tr);
      }
      table.appendChild(tbody);
      body.appendChild(table);
    }

    const footer = document.createElement("div");
    footer.className = "dialog-footer";
    footer.innerHTML = `<button class="dialog-btn dialog-btn-primary btn-new">New Provider</button>`;
    body.appendChild(footer);

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
      <div class="providers-confirm">
        <span class="providers-confirm-text">Delete provider <strong>${esc(name)}</strong>?</span>
        <div class="dialog-footer" style="border:none;padding:0;background:none">
          <button class="dialog-btn dialog-btn-secondary confirm-cancel">Cancel</button>
          <button class="dialog-btn dialog-btn-danger confirm-ok">Delete</button>
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
      <div class="dialog-title" style="margin-bottom:4px">${isEdit ? "Edit" : "New"} Provider</div>
      <div class="dialog-field">
        <label class="dialog-label">Name</label>
        <input class="dialog-input f-name" type="text" value="${escAttr(p.name)}" ${isEdit ? "readonly" : ""} />
      </div>
      <div class="dialog-field">
        <label class="dialog-label">Description</label>
        <input class="dialog-input f-desc" type="text" value="${escAttr(p.description)}" />
      </div>
      <div class="dialog-field">
        <label class="dialog-label">Command (binary path or name)</label>
        <input class="dialog-input f-cmd" type="text" value="${escAttr(p.command)}" />
      </div>
      <div class="dialog-field">
        <label class="dialog-label">Default Args (space-separated)</label>
        <input class="dialog-input f-args" type="text" value="${escAttr(p.default_args.join(" "))}" />
      </div>
      <div class="dialog-field">
        <label class="dialog-label">Prompt Format</label>
        <div class="f-format-slot"></div>
      </div>
      <div class="dialog-field flag-row" ${p.prompt_format !== "Flag" ? `style="display:none"` : ""}>
        <label class="dialog-label">Flag (e.g. --prompt)</label>
        <input class="dialog-input f-flag" type="text" value="${escAttr(p.prompt_flag)}" />
      </div>
      <label class="dialog-field" style="flex-direction:row;align-items:center;gap:8px">
        <input class="f-dispatch" type="checkbox" ${p.dispatchable ? "checked" : ""} />
        <span class="dialog-label" style="margin:0">Dispatchable (available as agent provider)</span>
      </label>
      <div class="dialog-field">
        <label class="dialog-label">Agent Dir (e.g. .my-ai/agents)</label>
        <input class="dialog-input f-agentdir" type="text" value="${escAttr(p.agent_dir ?? "")}" />
      </div>
      <div class="dialog-footer" style="border:none;padding:0;background:none;margin-top:6px">
        <button class="dialog-btn dialog-btn-secondary btn-cancel">Cancel</button>
        <button class="dialog-btn dialog-btn-primary btn-save">Save</button>
      </div>
    `;

    const cmdInput = body.querySelector<HTMLInputElement>(".f-cmd");
    if (cmdInput) attachPathPicker(cmdInput, { directory: false, title: "Select provider binary" });
    const agentDirInput = body.querySelector<HTMLInputElement>(".f-agentdir");
    if (agentDirInput) attachPathPicker(agentDirInput, { title: "Select agent directory" });

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
