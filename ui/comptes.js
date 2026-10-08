// Onglet « Comptes » : clés d'API des plateformes de trading (voir src-tauri/src/accounts.rs).
// Utilise $, call et toast d'app.js. Aucun secret ne revient jamais du serveur.
(() => {
  let page = { platforms: [], accounts: [] };
  let editing = null;

  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const date = (ms) => (ms ? new Date(ms).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" }) : "—");

  async function refresh() {
    let p;
    try { p = await call("list_accounts"); } catch { return; }
    page = p;
    const byId = Object.fromEntries(p.accounts.map((a) => [a.platform, a]));
    $("#acc-table tbody").innerHTML = p.platforms.map((pl) => {
      const a = byId[pl.id] || {};
      return `<tr data-id="${esc(pl.id)}">
        <td><b>${esc(pl.name)}</b></td>
        <td>${a.connected ? '<span class="badge gold">connecté</span>' : '<span class="badge">non connecté</span>'}</td>
        <td class="muted">${a.connected ? esc(a.key_hint || "masquée") : "—"}</td>
        <td class="muted">${date(a.saved_at)}</td>
        <td style="text-align:right;white-space:nowrap">
          <button type="button" class="ghost small" data-act="edit">${a.connected ? "Remplacer" : "Connecter"}</button>
          ${a.connected ? '<button type="button" class="danger small" data-act="del">Supprimer</button>' : ""}
        </td></tr>`;
    }).join("");
  }

  function openForm(id) {
    const pl = page.platforms.find((x) => x.id === id);
    if (!pl) return;
    editing = pl;
    $("#acc-form-title").textContent = `Connecter ${pl.name}`;
    $("#acc-form-help").innerHTML = `Crée la clé sur <b>${esc(pl.keys_url)}</b>, puis colle-la ici. Une clé déjà enregistrée est remplacée.`;
    $("#acc-fields").innerHTML = pl.fields.map((f) => f.multiline
      ? `<label style="grid-column:span 2">${esc(f.label)}<textarea name="${esc(f.id)}" rows="4" spellcheck="false" autocomplete="off"></textarea></label>`
      : `<label>${esc(f.label)}<input name="${esc(f.id)}" type="${f.secret ? "password" : "text"}" spellcheck="false" autocomplete="off"></label>`).join("");
    $("#acc-form").hidden = false;
    $("#acc-fields input, #acc-fields textarea")?.focus();
  }

  function closeForm() {
    $("#acc-form").reset();
    $("#acc-fields").innerHTML = ""; // les secrets saisis ne restent pas dans la page
    $("#acc-form").hidden = true;
    editing = null;
  }

  $("#acc-table").addEventListener("click", async (e) => {
    const b = e.target.closest("button[data-act]");
    if (!b) return;
    const id = b.closest("tr").dataset.id;
    const pl = page.platforms.find((x) => x.id === id);
    if (b.dataset.act === "edit") return openForm(id);
    if (!confirm(`Supprimer la clé ${pl.name} du Gestionnaire d'identification de Windows ?`)) return;
    try {
      await call("delete_account", { platform: id });
      toast(`Clé ${pl.name} supprimée`);
    } catch { /* motif déjà affiché par call() */ }
    refresh();
  });

  $("#acc-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!editing) return;
    const fields = {};
    editing.fields.forEach((f) => (fields[f.id] = $("#acc-form").elements[f.id].value));
    const name = editing.name;
    let r;
    try {
      r = await call("save_account", { platform: editing.id, fields });
    } catch { return; } // refus affiché par call() ; la saisie reste pour correction
    closeForm();
    toast(`${name} connecté (clé ${r.key_hint || "masquée"})`);
    refresh();
  });
  $("#acc-cancel").onclick = closeForm;

  $("#tabs").addEventListener("click", (e) => e.target.dataset.view === "accounts" && refresh());
})();
