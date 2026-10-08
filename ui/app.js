// PaperTrading — interface. Aucune dépendance, aucun appel réseau direct :
// tout passe par les commandes du serveur Rust.
"use strict";

const TAURI = window.__TAURI__;
const invoke = (cmd, args) => TAURI.core.invoke(cmd, args);
const listen = (ev, fn) => TAURI.event.listen(ev, fn);

// Paires en euros achetables sur Bitvavo (voir crates/pt-data/univers.txt).
const COMMON_SYMBOLS = ["TOP50", "BTCEUR", "ETHEUR", "XRPEUR", "SOLEUR", "ADAEUR", "SUIEUR", "LINKEUR", "DOGEEUR", "AVAXEUR", "DOTEUR"];
const DEFAULT_SYMBOLS = ["BTCEUR", "ETHEUR", "SOLEUR", "XRPEUR", "ADAEUR"];

const state = { presets: [], settings: null, view: "portfolios", detailId: null, comparing: false };

// ---------- utilitaires ----------
const $ = (sel, root = document) => root.querySelector(sel);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const nf = (d) => new Intl.NumberFormat("fr-FR", { minimumFractionDigits: d, maximumFractionDigits: d });
const money = (v) => (v == null || !isFinite(v) ? "—" : nf(2).format(v) + " €");
const num = (v, d = 2) => (v == null || !isFinite(v) ? "—" : nf(d).format(v));
const pct = (v, d = 1) => (v == null || !isFinite(v) ? "—" : (v > 0 ? "+" : "") + nf(d).format(v) + " %");
const cls = (v) => (v > 0 ? "pos" : v < 0 ? "neg" : "");
const signed = (v, f = pct) => `<span class="${cls(v)}">${f(v)}</span>`;
function price(v) {
  if (v == null || !isFinite(v)) return "—";
  const d = v >= 1000 ? 2 : v >= 1 ? 4 : v >= 0.01 ? 5 : 8;
  return nf(d).format(v);
}
const date = (ms) => (ms ? new Date(ms).toLocaleDateString("fr-FR", { year: "numeric", month: "2-digit", day: "2-digit" }) : "—");
const dateTime = (ms) => (ms ? new Date(ms).toLocaleString("fr-FR", { year: "2-digit", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }) : "—");
const preset = (id) => state.presets.find((p) => p.id === id);

let toastTimer;
function toast(msg, isErr = false) {
  const t = $("#toast");
  t.textContent = msg;
  t.className = "show" + (isErr ? " err" : "");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.className = ""), isErr ? 7000 : 3500);
}
async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    toast(String(e), true);
    throw e;
  }
}

function distance(d) {
  if (!d) return "";
  return d.type === "atr" ? `${num(d.value, 1)} ATR` : `${num(d.value, 1)} %`;
}
function exitsText(p) {
  const e = p.exits;
  const parts = [];
  if (e.stop) parts.push(`stop ${distance(e.stop)}`);
  if (e.trailing) parts.push(`stop suiveur ${distance(e.trailing.distance)}${e.trailing.activation_pct > 0 ? ` dès +${num(e.trailing.activation_pct, 1)} %` : ""}`);
  if (e.take_profit) parts.push(e.take_profit.type === "r_multiple" ? `objectif ${num(e.take_profit.value, 1)}× le risque` : `objectif +${num(e.take_profit.value, 1)} %`);
  if (e.max_bars) parts.push(`durée max ${e.max_bars} bougies`);
  if (!parts.length) parts.push("ne vend jamais");
  return parts.join(" · ");
}
function sizingText(p) {
  const s = p.sizing;
  let t = s.type === "risk" ? `risque ${num(s.risk_pct, 1)} % du capital par trade (position ≤ ${num(s.max_position_pct, 0)} %)`
    : s.type === "fixed" ? `${num(s.position_pct, 0)} % du capital par ${p.pyramid ? "couche" : "position"}`
    : "parts égales entre les symboles";
  if (p.pyramid) t += ` · renforcement tous les −${num(p.pyramid.step_pct, 0)} %, ${p.pyramid.max_layers} couches max`;
  if (p.trend_sma) t += ` · seulement au-dessus de la SMA ${p.trend_sma}`;
  return t;
}
function presetOptions(selectEl, includeBenchmark = true) {
  const families = [...new Set(state.presets.map((p) => p.family))];
  selectEl.innerHTML = families.map((f) => `<optgroup label="${esc(f)}">${state.presets
    .filter((p) => p.family === f && (includeBenchmark || p.rule.type !== "buy_hold"))
    .map((p) => `<option value="${esc(p.id)}">${esc(p.name)}</option>`).join("")}</optgroup>`).join("");
}

// ---------- sélecteur de symboles ----------
function symbolPicker(el, initial) {
  const chosen = new Set(initial);
  const extra = new Set(initial.filter((s) => !COMMON_SYMBOLS.includes(s)));
  function render() {
    const all = [...COMMON_SYMBOLS, ...extra];
    el.innerHTML = all.map((s) => `<span class="chip ${chosen.has(s) ? "on" : ""}" data-s="${esc(s)}">${esc(s.replace(/EUR$/, ""))}</span>`).join("")
      + `<input placeholder="Autre : ex. NEAR ou NEAREUR" maxlength="20">`;
  }
  el.addEventListener("click", (e) => {
    const s = e.target.dataset?.s;
    if (!s) return;
    chosen.has(s) ? chosen.delete(s) : chosen.add(s);
    render();
  });
  el.addEventListener("keydown", (e) => {
    if (e.target.tagName !== "INPUT" || e.key !== "Enter") return;
    e.preventDefault();
    let s = e.target.value.trim().toUpperCase().replace(/-/g, "");
    if (!s) return;
    // Seules les paires en euros s'achètent : « NEAR » devient « NEAREUR ». Une crypto que
    // Bitvavo ne cote pas en euros est refusée, avec son motif, à la validation.
    if (!/EUR$|^TOP\d+$/.test(s)) s += "EUR";
    extra.add(s);
    chosen.add(s);
    render();
    el.querySelector("input").focus();
  });
  render();
  return { get: () => [...chosen], set: (list) => { chosen.clear(); list.forEach((s) => { chosen.add(s); if (!COMMON_SYMBOLS.includes(s)) extra.add(s); }); render(); } };
}

// ---------- graphique ----------
function lineChart(series, opts = {}) {
  const W = 1000, H = 300, L = 70, R = 14, T = 12, B = 26;
  const pts = series.flatMap((s) => s.points);
  if (pts.length < 2) return `<div class="empty">Pas encore assez de points pour tracer une courbe.</div>`;
  let x0 = Math.min(...pts.map((p) => p[0])), x1 = Math.max(...pts.map((p) => p[0]));
  let y0 = Math.min(...pts.map((p) => p[1])), y1 = Math.max(...pts.map((p) => p[1]));
  if (x1 === x0) x1 = x0 + 1;
  const pad = (y1 - y0) * 0.06 || Math.abs(y0) * 0.01 || 1;
  y0 -= pad; y1 += pad;
  const X = (x) => L + ((x - x0) / (x1 - x0)) * (W - L - R);
  const Y = (y) => T + (1 - (y - y0) / (y1 - y0)) * (H - T - B);
  const ticks = 5;
  let grid = "";
  for (let i = 0; i <= ticks; i++) {
    const v = y0 + ((y1 - y0) * i) / ticks;
    grid += `<line x1="${L}" x2="${W - R}" y1="${Y(v)}" y2="${Y(v)}"/><text x="${L - 6}" y="${Y(v) + 4}" text-anchor="end">${esc(opts.yFormat ? opts.yFormat(v) : num(v, 0))}</text>`;
  }
  for (let i = 0; i <= 4; i++) {
    const t = x0 + ((x1 - x0) * i) / 4;
    grid += `<text x="${X(t)}" y="${H - 6}" text-anchor="${i === 0 ? "start" : i === 4 ? "end" : "middle"}">${esc(x1 - x0 < 3 * 86400000 ? dateTime(t) : date(t))}</text>`;
  }
  let split = "";
  if (opts.split && opts.split > x0 && opts.split < x1) {
    split = `<line x1="${X(opts.split)}" x2="${X(opts.split)}" y1="${T}" y2="${H - B}" stroke="#7aa2f7" stroke-dasharray="4 4"/>`
      + `<text x="${X(opts.split) + 6}" y="${T + 12}" fill="#7aa2f7">hors échantillon →</text>`;
  }
  const marks = (opts.markers || []).filter((m) => m.time >= x0 && m.time <= x1).map((m) => {
    const x = X(m.time), y = Y(m.value), buy = m.side === "Buy", d = buy ? 1 : -1;
    const pts = `${x.toFixed(1)},${(y - 7 * d).toFixed(1)} ${(x - 5).toFixed(1)},${(y + 2 * d).toFixed(1)} ${(x + 5).toFixed(1)},${(y + 2 * d).toFixed(1)}`;
    return `<polygon class="mark ${buy ? "buy" : "sell"}" points="${pts}"><title>${esc(m.label || "")}</title></polygon>`;
  }).join("");
  const lines = series.map((s) => `<polyline fill="none" stroke="${s.color}" stroke-width="${s.width || 1.8}" stroke-linejoin="round" points="${s.points.map((p) => `${X(p[0]).toFixed(1)},${Y(p[1]).toFixed(1)}`).join(" ")}"/>`).join("");
  const legend = `<div class="legend">${series.map((s) => `<span><i style="background:${s.color}"></i>${esc(s.name)}</span>`).join("")}</div>`;
  return legend + `<svg class="chart" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none"><g class="grid">${grid}</g>${split}${lines}<g class="marks">${marks}</g></svg>`;
}

// ---------- navigation ----------
function show(view) {
  state.view = view;
  document.querySelectorAll(".view").forEach((v) => (v.hidden = v.id !== `view-${view}`));
  document.querySelectorAll("#tabs button").forEach((b) => b.classList.toggle("active", b.dataset.view === view || (view === "detail" && b.dataset.view === "portfolios")));
  if (view === "portfolios") refreshPortfolios();
  if (view === "detail") refreshDetail();
}
$("#tabs").addEventListener("click", (e) => e.target.dataset.view && show(e.target.dataset.view));

// ---------- moteur ----------
function renderEngine(st) {
  const dot = $("#engine-dot");
  dot.className = "dot " + (st.last_error ? "err" : st.busy ? "busy" : st.running ? "on" : "");
  const last = st.last_tick ? new Date(st.last_tick).toLocaleTimeString("fr-FR") : "jamais";
  $("#engine-text").textContent = st.elsewhere
    ? "Mode direct assuré par une autre fenêtre · affichage seul"
    : st.running
    ? (st.busy ? "Mode direct : passage en cours…" : `Mode direct actif · dernier passage ${last}`)
    : "Mode direct arrêté";
  $("#engine-text").title = st.elsewhere
    ? "Une autre fenêtre de PaperTrading fait avancer les portefeuilles. Celle-ci affiche la même base, et prendra le relais si l'autre se ferme."
    : st.last_error || "";
  if (st.elsewhere) dot.className = "dot on";
  const btn = $("#engine-toggle");
  btn.hidden = !!st.elsewhere;
  btn.textContent = st.running ? "Arrêter" : "Démarrer";
  btn.onclick = async () => renderEngine(await call("set_engine_running", { running: !st.running }));
}

// ---------- portefeuilles ----------
let npPicker;
async function refreshPortfolios() {
  const list = await call("list_portfolios");
  const el = $("#portfolio-list");
  if (!list.length) {
    el.innerHTML = `<div class="empty">Aucun portefeuille. Crée-en un, ou commence par le <b>Comparateur</b> pour voir quelles stratégies tiennent la route sur l'historique.</div>`;
    return;
  }
  state.list = list;
  renderPortfolioTable();
}
const SORT_KEYS = {
  name: (p) => p.name.toLowerCase(), tf: (p) => p.timeframe, equity: (p) => p.equity, ret: (p) => p.return_pct,
  bench: (p) => p.benchmark_return_pct, diff: (p) => p.return_pct - p.benchmark_return_pct,
  pnl: (p) => p.realized_pnl + p.unrealized_pnl, trades: (p) => p.trades, win: (p) => p.win_rate_pct ?? -1, open: (p) => p.open_positions,
};
state.listSort = { key: "ret", dir: -1 };
state.listFilter = { q: "", tf: "", family: "", top: "" };
function filteredPortfolios() {
  const f = state.listFilter, q = f.q.trim().toLowerCase();
  let rows = state.list.filter((p) => (!f.tf || p.timeframe === f.tf) && (!f.family || p.family === f.family)
    && (!q || (p.name + " " + p.preset_name + " " + p.symbols.join(" ")).toLowerCase().includes(q)));
  if (f.top === "best") rows = rows.sort((a, b) => b.return_pct - a.return_pct).slice(0, 10);
  if (f.top === "worst") rows = rows.sort((a, b) => a.return_pct - b.return_pct).slice(0, 10);
  const k = SORT_KEYS[state.listSort.key], dir = state.listSort.dir;
  rows.sort((a, b) => { const x = k(a), y = k(b); return (x < y ? -1 : x > y ? 1 : 0) * dir; });
  return rows;
}
function renderPortfolioTable() {
  const list = state.list, el = $("#portfolio-list"), f = state.listFilter;
  const uniq = (fn) => [...new Set(list.map(fn))].sort();
  const opt = (vals, cur) => vals.map((v) => `<option value="${esc(v)}"${v === cur ? " selected" : ""}>${esc(v)}</option>`).join("");
  const rows = filteredPortfolios();
  const tot = list.reduce((a, p) => a + p.equity - p.initial_cash, 0);
  const winners = list.filter((p) => p.return_pct > 0).length, beat = list.filter((p) => p.return_pct > p.benchmark_return_pct).length;
  const th = (key, label, n = true) => `<th class="sortable${n ? " num" : ""}${state.listSort.key === key ? " sorted" : ""}" data-sort="${key}">${label}${state.listSort.key === key ? (state.listSort.dir < 0 ? " ▼" : " ▲") : ""}</th>`;
  el.className = "";
  el.innerHTML = `<div class="panel"><div class="grid-4">
      <div class="kpi"><div class="v">${list.length}</div><div class="l">portefeuilles</div></div>
      <div class="kpi"><div class="v ${cls(tot)}">${money(tot)}</div><div class="l">P&L total (réalisé + latent)</div></div>
      <div class="kpi"><div class="v">${winners} / ${list.length}</div><div class="l">en gain</div></div>
      <div class="kpi"><div class="v">${beat} / ${list.length}</div><div class="l">battent « acheter et garder »</div></div>
    </div></div>
    <div class="toolbar">
      <input id="pf-q" placeholder="Rechercher (nom, stratégie, symbole)" value="${esc(f.q)}">
      <select id="pf-tf"><option value="">Toutes unités</option>${opt(uniq((p) => p.timeframe), f.tf)}</select>
      <select id="pf-family"><option value="">Toutes familles</option>${opt(uniq((p) => p.family), f.family)}</select>
      <div class="seg"><button class="ghost small${f.top === "" ? " on" : ""}" data-top="">Tous</button><button class="ghost small${f.top === "best" ? " on" : ""}" data-top="best">10 meilleurs</button><button class="ghost small${f.top === "worst" ? " on" : ""}" data-top="worst">10 pires</button></div>
      <span class="muted small" id="pf-count">${rows.length} affiché(s)</span>
    </div>
    <div class="table-wrap"><table id="pf-table"><tr>${th("name", "Portefeuille", false)}${th("tf", "Unité", false)}${th("equity", "Valeur")}${th("ret", "Rendement")}${th("bench", "Acheter et garder")}${th("diff", "Écart")}${th("pnl", "P&L")}${th("trades", "Trades")}${th("win", "% gagnants")}${th("open", "Ouvertes")}<th></th></tr>
    ${rows.map((p) => `<tr class="pf-row" data-ret="${p.return_pct}"><td><a href="#" data-act="detail" data-id="${p.id}">${esc(p.name)}</a><div class="muted small">${esc(p.preset_name)} · ${esc(p.family)}${p.active ? "" : " · en pause"}${p.last_error ? ` · <span class="neg" title="${esc(p.last_error)}">erreur</span>` : ""}${p.waiting ? ` · <span title="${esc(p.waiting)}">en attente</span>` : ""}</div></td>
      <td>${esc(p.timeframe)}</td><td class="num">${money(p.equity)}</td><td class="num">${signed(p.return_pct, (v) => pct(v, 2))}</td><td class="num">${signed(p.benchmark_return_pct, (v) => pct(v, 2))}</td>
      <td class="num">${signed(p.return_pct - p.benchmark_return_pct, (v) => pct(v, 2))}</td><td class="num">${signed(p.realized_pnl + p.unrealized_pnl, money)}</td><td class="num">${p.trades}</td><td class="num">${p.win_rate_pct != null ? num(p.win_rate_pct, 0) + " %" : "—"}</td><td class="num">${p.open_positions}</td>
      <td class="actions"><button class="ghost small" data-act="detail" data-id="${p.id}">Détail</button><button class="ghost small" data-act="toggle" data-id="${p.id}" data-active="${p.active}">${p.active ? "Pause" : "Reprendre"}</button><button class="danger small" data-act="delete" data-id="${p.id}" data-name="${esc(p.name)}">Suppr.</button></td></tr>`).join("")}
    </table></div>`;
  const q = $("#pf-q");
  q.oninput = () => { f.q = q.value; const pos = q.selectionStart; renderPortfolioTable(); const n = $("#pf-q"); n.focus(); n.setSelectionRange(pos, pos); };
  $("#pf-tf").onchange = (e) => { f.tf = e.target.value; renderPortfolioTable(); };
  $("#pf-family").onchange = (e) => { f.family = e.target.value; renderPortfolioTable(); };
}
$("#portfolio-list").addEventListener("click", async (e) => {
  const h = e.target.closest("th[data-sort]");
  if (h) { const k = h.dataset.sort; state.listSort = { key: k, dir: state.listSort.key === k ? -state.listSort.dir : (k === "name" || k === "tf" ? 1 : -1) }; return renderPortfolioTable(); }
  const b = e.target.closest("button, a[data-act]");
  if (!b) return;
  e.preventDefault();
  if (b.dataset.top != null) { state.listFilter.top = b.dataset.top; return renderPortfolioTable(); }
  const id = Number(b.dataset.id);
  if (b.dataset.act === "detail") { state.detailId = id; show("detail"); }
  if (b.dataset.act === "toggle") { await call("set_portfolio_active", { id, active: b.dataset.active !== "true" }); refreshPortfolios(); }
  if (b.dataset.act === "delete") {
    if (!confirm(`Supprimer « ${b.dataset.name} » et tout son historique ? Cette action est définitive.`)) return;
    await call("delete_portfolio", { id });
    toast("Portefeuille supprimé.");
    refreshPortfolios();
  }
});
$("#new-portfolio-btn").onclick = () => { $("#new-portfolio").hidden = false; $("#new-portfolio [name=cash]").value = state.settings.default_cash; };
$("#np-cancel").onclick = () => ($("#new-portfolio").hidden = true);
$("#np-preset").addEventListener("change", () => {
  const p = preset($("#np-preset").value);
  $("#np-hint").textContent = p ? `${p.description} Sorties : ${exitsText(p)}. Taille : ${sizingText(p)}.` : "";
});
$("#new-portfolio").addEventListener("submit", async (e) => {
  e.preventDefault();
  const f = e.target;
  const btn = f.querySelector("button[type=submit]");
  btn.disabled = true;
  try {
    await call("create_portfolio", { req: { name: f.name.value, preset_id: f.preset.value, symbols: npPicker.get(), cash: Number(f.cash.value) } });
    toast("Portefeuille créé. Il démarrera à la prochaine clôture de bougie.");
    f.hidden = true;
    f.name.value = "";
    refreshPortfolios();
  } finally { btn.disabled = false; }
});

// ---------- détail ----------
async function refreshDetail() {
  if (state.detailId == null) return show("portfolios");
  const d = await call("portfolio_detail", { id: state.detailId });
  const s = d.summary;
  const curve = d.curve;
  const chart = lineChart([
    { name: "Acheter et garder", color: "#8b93a6", points: curve.map((p) => [p.time, p.benchmark]) },
    { name: s.preset_name, color: "#e3ba55", width: 2.2, points: curve.map((p) => [p.time, p.equity]) },
  ], { yFormat: (v) => nf(0).format(v), markers: fillMarkers(d.fills, curve) });
  state.detail = d;
  state.tradeFilter = { sym: "", res: "", reason: "", from: "", to: "", key: "exit_time", dir: -1 };
  $("#detail").innerHTML = `
    <div class="section-head" style="margin-top:10px"><div><h1>${esc(s.name)}</h1>
      <p class="muted">${esc(d.preset.name)} · ${esc(s.timeframe)} · ${s.symbols.map(esc).join(", ")} · créé le ${dateTime(s.created_at)}</p>
      <p class="muted small">${esc(d.preset.description)} Sorties : ${esc(exitsText(d.preset))}. Taille : ${esc(sizingText(d.preset))}.</p></div></div>
    ${s.waiting ? `<div class="notice">${esc(s.waiting)}</div>` : ""}
    <div class="panel"><div class="grid-5">
      <div class="kpi"><div class="v">${money(s.equity)}</div><div class="l">valeur</div></div>
      <div class="kpi"><div class="v ${cls(s.return_pct)}">${pct(s.return_pct, 2)}</div><div class="l">stratégie</div></div>
      <div class="kpi"><div class="v ${cls(s.benchmark_return_pct)}">${pct(s.benchmark_return_pct, 2)}</div><div class="l">acheter et garder</div></div>
      <div class="kpi"><div class="v ${cls(s.realized_pnl)}">${money(s.realized_pnl)}</div><div class="l">gains réalisés (frais déduits)</div></div>
      <div class="kpi"><div class="v">${money(s.fees_paid)}</div><div class="l">frais payés</div></div>
    </div></div>
    <div class="panel">${chart}<p class="muted small"><span class="pos">▲</span> achat · <span class="neg">▼</span> vente — ${d.fills.length} exécution(s) placée(s) sur la courbe à leur date (survole un marqueur pour le détail).</p></div>
    <div class="panel"><h2>Analyse du P&L</h2>${pnlAnalysis(s, d.trades)}</div>
    <div class="panel"><h2>Positions ouvertes</h2>${d.positions.length ? `<div class="table-wrap"><table>
      <tr><th>Symbole</th><th class="num">Quantité</th><th class="num">Prix moyen</th><th class="num">Prix actuel</th><th class="num">Valeur</th><th class="num">Latent</th><th class="num">Stop</th><th class="num">Objectif</th><th class="num">Couches</th><th>Entrée</th></tr>
      ${d.positions.map((p) => `<tr><td>${esc(p.symbol)}</td><td class="num">${num(p.qty, 6)}</td><td class="num">${price(p.avg_price)}</td><td class="num">${price(p.mark)}</td><td class="num">${money(p.value)}</td>
        <td class="num">${signed(p.unrealized_pnl, money)} (${signed(p.unrealized_pct)})</td><td class="num">${price(p.stop)}</td><td class="num">${price(p.take_profit)}</td><td class="num">${p.layers}</td><td>${dateTime(p.entry_time)}</td></tr>`).join("")}
      </table></div>` : `<p class="muted">Aucune position ouverte.</p>`}</div>
    <div class="panel"><h2>Historique des trades (${d.trades.length})</h2>${tradeFilters(d.trades)}<div id="trades-box"></div></div>
    <details class="panel" id="skipped-panel"><summary>Signaux écartés (chargement…)</summary><div id="skipped-box"></div></details>
    <details class="panel"><summary>Toutes les exécutions (${d.fills.length})</summary>${d.fills.length ? `<div class="table-wrap"><table>
      <tr><th>Date</th><th>Symbole</th><th>Sens</th><th class="num">Quantité</th><th class="num">Prix</th><th class="num">Frais</th><th class="num">Trésorerie</th><th>Motif</th></tr>
      ${d.fills.map((f) => `<tr><td>${dateTime(f.time)}</td><td>${esc(f.symbol)}</td><td>${f.side === "Buy" ? "Achat" : "Vente"}</td><td class="num">${num(f.qty, 6)}</td><td class="num">${price(f.price)}</td><td class="num">${money(f.fee)}</td><td class="num">${signed(f.cash_delta, money)}</td><td>${esc(f.reason)}</td></tr>`).join("")}
      </table></div>` : `<p class="muted">Aucune exécution.</p>`}</details>`;
  renderTrades();
  loadDecisions(state.detailId);
}
function fillMarkers(fills, curve) {
  if (!curve.length) return [];
  const eqAt = (t) => {
    let lo = 0, hi = curve.length - 1;
    if (t <= curve[0].time) return curve[0].equity;
    if (t >= curve[hi].time) return curve[hi].equity;
    while (hi - lo > 1) { const m = (lo + hi) >> 1; if (curve[m].time <= t) lo = m; else hi = m; }
    const a = curve[lo], b = curve[hi];
    return a.equity + ((b.equity - a.equity) * (t - a.time)) / (b.time - a.time || 1);
  };
  return fills.map((f) => ({ time: f.time, value: eqAt(f.time), side: f.side, label: `${f.side === "Buy" ? "Achat" : "Vente"} ${f.symbol} · ${num(f.qty, 6)} @ ${price(f.price)} · ${dateTime(f.time)}${f.reason ? " · " + f.reason : ""}` }));
}
function pnlAnalysis(s, trades) {
  const wins = trades.filter((t) => t.pnl > 0), losses = trades.filter((t) => t.pnl <= 0);
  const sum = (a) => a.reduce((x, t) => x + t.pnl, 0);
  const gw = sum(wins), gl = sum(losses), avg = trades.length ? sum(trades) / trades.length : null;
  const best = trades.reduce((a, t) => (!a || t.pnl > a.pnl ? t : a), null), worst = trades.reduce((a, t) => (!a || t.pnl < a.pnl ? t : a), null);
  const bySym = {};
  trades.forEach((t) => { const o = (bySym[t.symbol] ||= { n: 0, w: 0, pnl: 0 }); o.n++; o.pnl += t.pnl; if (t.pnl > 0) o.w++; });
  const syms = Object.entries(bySym).sort((a, b) => b[1].pnl - a[1].pnl);
  const total = s.realized_pnl + s.unrealized_pnl;
  return `<div class="grid-5">
      <div class="kpi"><div class="v ${cls(total)}" id="pnl-total">${money(total)}</div><div class="l">P&L total (réalisé + latent)</div></div>
      <div class="kpi"><div class="v ${cls(s.realized_pnl)}">${money(s.realized_pnl)}</div><div class="l">réalisé</div></div>
      <div class="kpi"><div class="v ${cls(s.unrealized_pnl)}">${money(s.unrealized_pnl)}</div><div class="l">latent (positions ouvertes)</div></div>
      <div class="kpi"><div class="v ${cls(avg)}">${money(avg)}</div><div class="l">espérance par trade</div></div>
      <div class="kpi"><div class="v">${gl < 0 ? num(gw / -gl, 2) : gw > 0 ? "∞" : "—"}</div><div class="l">facteur de profit</div></div>
      <div class="kpi"><div class="v">${trades.length ? `${wins.length} / ${losses.length}` : "—"}</div><div class="l">gagnants / perdants</div></div>
      <div class="kpi"><div class="v pos">${wins.length ? money(gw / wins.length) : "—"}</div><div class="l">gain moyen</div></div>
      <div class="kpi"><div class="v neg">${losses.length ? money(gl / losses.length) : "—"}</div><div class="l">perte moyenne</div></div>
      <div class="kpi"><div class="v ${cls(best?.pnl)}">${best ? money(best.pnl) : "—"}</div><div class="l">meilleur trade${best ? " · " + esc(best.symbol) : ""}</div></div>
      <div class="kpi"><div class="v ${cls(worst?.pnl)}">${worst ? money(worst.pnl) : "—"}</div><div class="l">pire trade${worst ? " · " + esc(worst.symbol) : ""}</div></div>
    </div>
    <p class="muted small">Frais payés : ${money(s.fees_paid)}, déjà déduits des gains.</p>
    ${syms.length ? `<h3>P&L par symbole</h3><div class="table-wrap"><table><tr><th>Symbole</th><th class="num">Trades</th><th class="num">% gagnants</th><th class="num">P&L</th></tr>
      ${syms.map(([k, o]) => `<tr><td>${esc(k)}</td><td class="num">${o.n}</td><td class="num">${num((o.w / o.n) * 100, 0)} %</td><td class="num">${signed(o.pnl, money)}</td></tr>`).join("")}</table></div>` : ""}
    ${groupTable("P&L par jour", trades, (t) => date(t.exit_time), (a, b) => b.localeCompare(a))}
    ${groupTable("P&L par heure de sortie (UTC)", trades, (t) => String(new Date(t.exit_time).getUTCHours()).padStart(2, "0") + " h", (a, b) => a.localeCompare(b))}
    ${groupTable("P&L par motif de sortie", trades, (t) => t.exit_reason || "—", null)}`;
}
function groupTable(title, trades, key, order) {
  if (!trades.length) return "";
  const g = {};
  trades.forEach((t) => { const o = (g[key(t)] ||= { n: 0, w: 0, pnl: 0 }); o.n++; o.pnl += t.pnl; if (t.pnl > 0) o.w++; });
  const rows = Object.entries(g).sort(order ? (x, y) => order(x[0], y[0]) : (x, y) => y[1].pnl - x[1].pnl);
  return `<h3>${esc(title)}</h3><div class="table-wrap"><table><tr><th></th><th class="num">Trades</th><th class="num">% gagnants</th><th class="num">P&L</th></tr>
      ${rows.map(([k, o]) => `<tr><td>${esc(k)}</td><td class="num">${o.n}</td><td class="num">${num((o.w / o.n) * 100, 0)} %</td><td class="num">${signed(o.pnl, money)}</td></tr>`).join("")}</table></div>`;
}
function tradeFilters(trades) {
  if (!trades.length) return "";
  const opt = (vals) => [...new Set(vals)].sort().map((v) => `<option value="${esc(v)}">${esc(v)}</option>`).join("");
  return `<div class="toolbar">
    <select data-f="sym"><option value="">Tous symboles</option>${opt(trades.map((t) => t.symbol))}</select>
    <select data-f="res"><option value="">Gagnants et perdants</option><option value="win">Gagnants</option><option value="loss">Perdants</option></select>
    <select data-f="reason"><option value="">Tous motifs</option>${opt(trades.map((t) => t.exit_reason))}</select>
    <label>Du<input type="date" data-f="from"></label><label>Au<input type="date" data-f="to"></label>
  </div>`;
}
function renderTrades() {
  const box = $("#trades-box");
  if (!box) return;
  const f = state.tradeFilter, all = state.detail.trades;
  const from = f.from ? new Date(f.from + "T00:00:00").getTime() : -Infinity, to = f.to ? new Date(f.to + "T23:59:59").getTime() : Infinity;
  const rows = all.filter((t) => (!f.sym || t.symbol === f.sym) && (!f.reason || t.exit_reason === f.reason)
    && (!f.res || (f.res === "win" ? t.pnl > 0 : t.pnl <= 0)) && t.exit_time >= from && t.exit_time <= to);
  rows.sort((a, b) => { const x = a[f.key], y = b[f.key]; return (x < y ? -1 : x > y ? 1 : 0) * f.dir; });
  const pnl = rows.reduce((a, t) => a + t.pnl, 0);
  state.tradeRows = rows;
  box.innerHTML = `<div id="trade-view"></div>` + (all.length ? `<p class="muted small">${rows.length} trade(s) affiché(s) · P&L de la sélection ${signed(pnl, money)}</p>` : "") + tradesTable(rows, 400, f);
}
$("#detail").addEventListener("change", (e) => { const k = e.target.dataset.f; if (k && state.tradeFilter) { state.tradeFilter[k] = e.target.value; renderTrades(); } });
$("#detail").addEventListener("click", (e) => {
  const r = e.target.closest("#trades-box tr.trade-row");
  if (r && state.tradeRows) { showTrade(state.tradeRows[+r.dataset.i]); return; }
  const h = e.target.closest("#trades-box th[data-sort]");
  if (!h) return;
  const f = state.tradeFilter, k = h.dataset.sort;
  f.dir = f.key === k ? -f.dir : -1; f.key = k; renderTrades();
});
function tradesTable(trades, limit = 400, sort) {
  if (!trades.length) return `<p class="muted">Aucun trade clos.</p>`;
  const rows = trades.slice(0, limit);
  const th = (key, label, n) => sort ? `<th class="sortable${n ? " num" : ""}${sort.key === key ? " sorted" : ""}" data-sort="${key}">${label}${sort.key === key ? (sort.dir < 0 ? " ▼" : " ▲") : ""}</th>` : `<th${n ? ' class="num"' : ""}>${label}</th>`;
  return `<div class="table-wrap"><table>
    <tr>${th("symbol", "Symbole")}${th("entry_time", "Entrée")}${th("exit_time", "Sortie")}<th class="num">Prix moyen</th><th class="num">Prix de sortie</th>${th("pnl", "Gain net", 1)}${th("return_pct", "%", 1)}${th("bars_held", "Bougies", 1)}${th("exit_reason", "Motif de sortie")}</tr>
    ${rows.map((t, i) => `<tr class="trade-row" data-i="${i}" title="Voir l'évolution du prix pendant ce trade"><td>${esc(t.symbol)}</td><td>${dateTime(t.entry_time)}</td><td>${dateTime(t.exit_time)}</td><td class="num">${price(t.avg_entry_price)}</td><td class="num">${price(t.exit_price)}</td>
      <td class="num">${signed(t.pnl, money)}</td><td class="num">${signed(t.return_pct, (v) => pct(v, 2))}</td><td class="num">${t.bars_held}</td><td>${esc(t.exit_reason)}</td></tr>`).join("")}
  </table></div>${trades.length > limit ? `<p class="muted small">${limit} premiers sur ${trades.length}.</p>` : ""}`;
}
$("#detail-back").onclick = () => show("portfolios");

// ---------- backtest ----------
let btPicker;
function metricRow(label, a, b, f, better) {
  const good = better == null ? "" : (better === "high" ? a > b : a < b) ? "pos" : "neg";
  return `<tr><td>${label}</td><td class="num ${good}">${f(a)}</td><td class="num">${f(b)}</td></tr>`;
}
function renderBacktest(r) {
  const m = r.metrics, b = r.benchmark;
  const chart = lineChart([
    { name: "Acheter et garder", color: "#8b93a6", points: r.benchmark_curve.map((p) => [p.time, p.equity]) },
    { name: r.preset_name, color: "#e3ba55", width: 2.2, points: r.curve.map((p) => [p.time, p.equity]) },
  ], { split: r.split_time, yFormat: (v) => nf(0).format(v) });
  const fees = m.start_equity ? (r.fees_paid / m.start_equity) * 100 : 0;
  $("#bt-result").innerHTML = `
    <div class="verdict-banner"><span class="verdict v-${r.verdict} big">${esc(verdictLabel(r.verdict))}</span>
      <div><div>${esc(r.verdict_reason)}</div><div class="muted small">${esc(r.symbols.join(", "))} · ${esc(r.timeframe)} · du ${date(r.start_time)} au ${date(r.end_time)} · hors échantillon depuis le ${date(r.split_time)}</div></div></div>
    <div class="panel">${chart}</div>
    <div class="metrics-grid">
      <div class="panel"><h2>Comparaison</h2><table>
        <tr><th></th><th class="num">Stratégie</th><th class="num">Acheter et garder</th></tr>
        ${metricRow("Rendement total", m.total_return_pct, b.total_return_pct, pct, "high")}
        ${metricRow("Rendement annualisé", m.cagr_pct, b.cagr_pct, pct, "high")}
        ${metricRow("Pire baisse depuis un sommet", m.max_drawdown_pct, b.max_drawdown_pct, pct, "low")}
        ${metricRow("Sharpe (rendement / risque)", m.sharpe, b.sharpe, (v) => num(v, 2), "high")}
        ${metricRow("Sharpe hors échantillon", r.oos.sharpe, r.benchmark_oos.sharpe, (v) => num(v, 2), "high")}
        ${metricRow("Rendement hors échantillon", r.oos.total_return_pct, r.benchmark_oos.total_return_pct, pct, "high")}
        ${metricRow("Volatilité annualisée", m.volatility_pct, b.volatility_pct, pct, "low")}
        ${metricRow("Temps investi", m.exposure_pct, b.exposure_pct, pct)}
        <tr><td>Achat au hasard, <b>même exposition</b></td><td class="num">${pct(r.matched_return_pct)}</td><td class="num muted">barre à battre</td></tr>
      </table></div>
      <div class="panel"><h2>Trades</h2><table>
        <tr><td>Nombre de trades</td><td class="num">${m.trades}</td></tr>
        <tr><td>Trades gagnants</td><td class="num">${pct(m.win_rate_pct, 1).replace("+", "")}</td></tr>
        <tr><td>Facteur de profit (gains / pertes)</td><td class="num">${m.profit_factor == null ? "—" : num(m.profit_factor, 2)}</td></tr>
        <tr><td>Gain moyen / perte moyenne</td><td class="num">${signed(m.avg_win_pct, (v) => pct(v, 2))} / ${signed(m.avg_loss_pct, (v) => pct(v, 2))}</td></tr>
        <tr><td>Espérance par trade</td><td class="num">${signed(m.expectancy_pct, (v) => pct(v, 2))}</td></tr>
        <tr><td>Durée moyenne</td><td class="num">${num(m.avg_bars_held, 1)} bougies</td></tr>
        <tr><td>Frais payés</td><td class="num">${money(r.fees_paid)} (${num(fees, 1)} % du capital)</td></tr>
        <tr><td>Même stratégie <b>sans aucun frais</b></td><td class="num">${signed(r.gross_return_pct)}</td></tr>
      </table></div>
    </div>
    <div class="panel"><h2>Trades clos (${r.trades.length})</h2>${tradesTable([...r.trades].reverse())}</div>`;
  if (r.validation) renderValidation(r.validation);
  else if (r.validation_error) $("#val-result").innerHTML = `<div class="notice err" style="margin-top:12px">Validation impossible : ${esc(r.validation_error)}</div>`;
  else $("#val-result").innerHTML = "";
}
const VERDICTS = {
  Solide: "Solide",
  Prometteuse: "Prometteuse",
  Hasard: "Indiscernable du hasard",
  AValider: "À valider",
  Perdante: "Perdante",
  Reference: "Référence",
  Insuffisant: "Trop peu de données",
};
function verdictLabel(v) {
  return VERDICTS[v] || v;
}
// Paramètres de validation lus dans le panneau « Est-ce un coup de chance ? ».
function validationParams(formSelector = "#val-form") {
  const f = $(formSelector);
  return { days: Number(f.vdays.value), window_days: Number(f.window.value), step_days: Number(f.step.value) };
}
// Un backtest est TOUJOURS accompagné de sa validation : c'est elle qui décide du verdict.
async function runBacktest(presetId, symbols, days, cash) {
  const buttons = [$("#bt-run"), $("#val-run")];
  buttons.forEach((b) => (b.disabled = true));
  $("#bt-run").textContent = "Calcul…";
  $("#val-result").innerHTML = "";
  $("#bt-result").innerHTML = `<div class="empty">Téléchargement de l'historique long (une seule fois, ensuite en cache), backtest de la période, puis une exécution par fenêtre de validation…</div>`;
  try {
    const r = await call("run_backtest", { req: { preset_id: presetId, symbols, days, cash, validation: validationParams() } });
    renderBacktest(r);
  } catch (e) {
    $("#bt-result").innerHTML = `<div class="notice err">${esc(e)}</div>`;
  } finally {
    buttons.forEach((b) => (b.disabled = false));
    $("#bt-run").textContent = "Lancer";
  }
}
$("#bt-form").addEventListener("submit", (e) => {
  e.preventDefault();
  const f = e.target;
  runBacktest(f.preset.value, btPicker.get(), Number(f.days.value), Number(f.cash.value));
});
function openBacktest(presetId, symbols) {
  show("backtest");
  $("#bt-preset").value = presetId;
  if (symbols) btPicker.set(symbols);
  const f = $("#bt-form");
  runBacktest(presetId, btPicker.get(), Number(f.days.value), Number(f.cash.value));
}

// ---------- validation sur fenêtres glissantes ----------
const ROBUSTNESS = {
  PasDeLaChance: { label: "Pas un coup de chance", cls: "Solide" },
  Prometteuse: { label: "Prometteuse, pas prouvée", cls: "Prometteuse" },
  CompatibleAvecLaChance: { label: "Compatible avec la chance", cls: "Perdante" },
  TropPeuDeFenetres: { label: "Trop peu de fenêtres", cls: "Insuffisant" },
};
const OUTCOME = { Win: "gagnée", Loss: "perdue", Tie: "nulle" };
function windowsChart(windows) {
  const W = 1000, H = 220, L = 54, R = 10, T = 12, B = 24;
  if (!windows.length) return "";
  const vals = windows.map((w) => w.excess_pct);
  const hi = Math.max(1, ...vals), lo = Math.min(-1, ...vals);
  const Y = (v) => T + ((hi - v) / (hi - lo)) * (H - T - B);
  const slot = (W - L - R) / windows.length;
  const bw = Math.max(2, slot * 0.72);
  let s = `<line x1="${L}" x2="${W - R}" y1="${Y(0)}" y2="${Y(0)}" stroke="#5b667d"/>`;
  for (const v of [hi, lo]) s += `<text x="${L - 6}" y="${Y(v) + 4}" text-anchor="end">${esc(pct(v, 0).replace(" %", " pts"))}</text>`;
  windows.forEach((w, i) => {
    const x = L + i * slot + (slot - bw) / 2;
    const y0 = Y(0), y1 = Y(w.excess_pct);
    const color = w.outcome === "Win" ? "#4cc38a" : w.outcome === "Loss" ? "#e5675f" : "#8b93a6";
    s += `<rect x="${x.toFixed(1)}" y="${Math.min(y0, y1).toFixed(1)}" width="${bw.toFixed(1)}" height="${Math.max(1, Math.abs(y1 - y0)).toFixed(1)}" fill="${color}"><title>${esc(date(w.start))} → ${esc(date(w.end - 1))} : ${esc(pct(w.excess_pct, 1).replace(" %", " points"))}</title></rect>`;
  });
  const ticks = Math.min(6, windows.length);
  for (let k = 0; k < ticks; k++) {
    const i = Math.round((k * (windows.length - 1)) / Math.max(1, ticks - 1));
    // Première et dernière étiquettes ancrées vers l'intérieur : sinon elles débordent du cadre.
    const anchor = k === 0 ? "start" : k === ticks - 1 ? "end" : "middle";
    const x = k === 0 ? L + i * slot : k === ticks - 1 ? L + (i + 1) * slot : L + i * slot + slot / 2;
    s += `<text x="${x.toFixed(1)}" y="${H - 6}" text-anchor="${anchor}">${esc(date(windows[i].start))}</text>`;
  }
  return `<div class="legend"><span><i style="background:#4cc38a"></i>bat la référence à exposition égale</span><span><i style="background:#e5675f"></i>fait moins bien</span><span>hauteur = écart en points de rendement</span></div>
    <svg class="chart" style="height:220px" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none">${s}</svg>`;
}
function renderValidation(r) {
  const v = ROBUSTNESS[r.verdict] || { label: r.verdict, cls: "Insuffisant" };
  const t = r.sign_test;
  const n = r.windows.length;
  $("#val-result").innerHTML = `
    <div class="verdict-banner" style="margin-top:12px"><span class="verdict v-${v.cls} big">${esc(v.label)}</span>
      <div><div>${esc(r.verdict_reason)}</div>
      <div class="muted small">${esc(r.preset_name)} · ${n} fenêtres de ${r.config.window_days} jours décalées de ${r.config.step_days} jours, du ${date(r.windows[0].start)} au ${date(r.windows[n - 1].end - 1)} · test sur une fenêtre sur ${r.stride} (${t.wins} gagnée(s), ${t.losses} perdue(s), ${t.ties} nulle(s), découpage le moins favorable)</div></div></div>
    <div class="grid-4" style="margin-bottom:10px">
      <div class="kpi"><div class="v">${num(t.p_value * 100, 1)} %</div><div class="l">probabilité à pile ou face</div></div>
      <div class="kpi"><div class="v">${num(r.p_adjusted * 100, 1)} %</div><div class="l">corrigée pour ${r.config.tested_strategies} stratégies essayées (seuil 5 %)</div></div>
      <div class="kpi"><div class="v">${r.positive_windows}/${n}</div><div class="l">fenêtres en gain</div></div>
      <div class="kpi"><div class="v ${cls(r.median_excess_pct)}">${pct(r.median_excess_pct, 1).replace(" %", " pts")}</div><div class="l">écart médian avec la référence à exposition égale</div></div>
    </div>
    ${windowsChart(r.windows)}
    <details><summary class="muted small">Détail des ${n} fenêtres</summary><div class="table-wrap"><table>
      <tr><th>Début</th><th>Fin</th><th class="num">Symboles</th><th class="num">Trades</th><th class="num">Exposition</th><th class="num">Stratégie</th><th class="num">Acheter-garder</th><th class="num">Réf. à expo. égale</th><th class="num">Écart</th><th class="num">Pire baisse</th><th class="num">Réf.</th><th>Résultat</th></tr>
      ${r.windows.map((w) => `<tr><td>${date(w.start)}</td><td>${date(w.end - 1)}</td><td class="num">${w.symbols.length}</td><td class="num">${w.trades}</td><td class="num">${num(w.exposure_pct, 0)} %</td>
        <td class="num">${signed(w.return_pct)}</td><td class="num muted">${pct(w.benchmark_return_pct)}</td><td class="num">${pct(w.matched_return_pct)}</td>
        <td class="num">${signed(w.excess_pct, (x) => pct(x, 1).replace(" %", " pts"))}</td><td class="num">${pct(w.max_drawdown_pct).replace("+", "")}</td><td class="num muted">${pct(w.benchmark_max_drawdown_pct).replace("+", "")}</td>
        <td><span class="verdict v-${w.outcome === "Win" ? "Solide" : w.outcome === "Loss" ? "Perdante" : "Insuffisant"}">${OUTCOME[w.outcome]}</span></td></tr>`).join("")}
    </table></div></details>`;
}
$("#val-form").addEventListener("submit", (e) => {
  e.preventDefault();
  const f = $("#bt-form");
  runBacktest(f.preset.value, btPicker.get(), Number(f.days.value), Number(f.cash.value));
});
$("#bt-preset").addEventListener("change", () => ($("#val-result").innerHTML = ""));
// ---------- comparateur ----------
let cmpPicker;
const RANK = { Solide: 0, Prometteuse: 1, Reference: 2, AValider: 3, Hasard: 4, Perdante: 5, Insuffisant: 6 };
const pAdj = (r) => (r.validation ? r.validation.p_adjusted : 1);
const pRaw = (r) => (r.validation ? r.validation.sign_test.p_value : 1);
$("#cmp-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  if (state.comparing) return;
  const f = e.target;
  const symbols = cmpPicker.get();
  const validation = validationParams("#cmp-form");
  state.comparing = true;
  $("#cmp-run").disabled = true;
  $("#cmp-progress").hidden = false;
  $("#cmp-result").innerHTML = "";
  try {
    const rows = await call("run_comparison", { req: { symbols, days: Number(f.days.value), cash: Number(f.cash.value), validation } });
    rows.sort((a, b) => (RANK[a.verdict] - RANK[b.verdict]) || (pAdj(a) - pAdj(b)) || (pRaw(a) - pRaw(b))
      || ((b.metrics.total_return_pct - b.matched_return_pct) - (a.metrics.total_return_pct - a.matched_return_pct)));
    const counts = rows.reduce((acc, r) => ((acc[r.verdict] = (acc[r.verdict] || 0) + 1), acc), {});
    const tested = rows.find((r) => r.validation)?.validation.config.tested_strategies ?? rows.length - 1;
    $("#cmp-result").innerHTML = `<div class="panel">
      <p><b>${counts.Solide || 0}</b> solide(s), <b>${counts.Prometteuse || 0}</b> prometteuse(s), <b>${counts.Hasard || 0}</b> indiscernable(s) du hasard, <b>${counts.Perdante || 0}</b> perdante(s), <b>${counts.Insuffisant || 0}</b> sans assez de données, sur ${rows.length} stratégies. Clique une ligne pour voir son backtest et ses fenêtres.</p>
      <div class="table-wrap" style="max-height:none"><table>
      <tr><th>Stratégie</th><th>UT</th><th class="num">Trades</th><th class="num">Rendement</th><th class="num">Acheter-garder</th><th class="num">Expo.</th><th class="num">Hasard à expo. égale</th><th class="num">Pire baisse</th><th class="num">Réf.</th><th class="num">Sans frais</th><th class="num">Fenêtres gagnées</th><th class="num">p corrigé</th><th>Verdict</th></tr>
      ${rows.map((r) => {
        const v = r.validation;
        const won = v ? `${v.sign_test.wins} / ${v.sign_test.wins + v.sign_test.losses}` : "—";
        const p = v ? `${num(v.p_adjusted * 100, 1)} %` : "—";
        return `<tr class="click" data-id="${esc(r.preset_id)}" title="${esc(r.verdict_reason)}">
        <td>${esc(r.preset_name)}</td><td>${esc(r.timeframe)}</td><td class="num">${r.error ? "—" : r.metrics.trades}</td>
        <td class="num">${signed(r.metrics.total_return_pct)}</td><td class="num muted">${pct(r.benchmark.total_return_pct)}</td>
        <td class="num">${num(r.metrics.exposure_pct, 0)} %</td><td class="num">${pct(r.matched_return_pct)}</td>
        <td class="num">${pct(r.metrics.max_drawdown_pct).replace("+", "")}</td><td class="num muted">${pct(r.benchmark.max_drawdown_pct).replace("+", "")}</td>
        <td class="num">${signed(r.gross_return_pct)}</td><td class="num">${won}</td><td class="num">${p}</td>
        <td><span class="verdict v-${r.error ? "Insuffisant" : r.verdict}">${esc(r.error ? "Erreur" : verdictLabel(r.verdict))}</span></td></tr>`;
      }).join("")}
      </table></div>
      <p class="muted small">« Hasard à expo. égale » : ce qu'un achat au hasard, investi la même part du temps, obtient en moyenne sur la période — c'est la barre à battre, pas « acheter et garder » à 100 %. « Fenêtres gagnées » : fenêtres indépendantes où la stratégie bat cette barre, sur tout l'historique de validation. « p corrigé » : probabilité de faire au moins aussi bien à pile ou face, multipliée par les ${tested} stratégies essayées. « Solide » exige p corrigé &lt; 5 %.</p>
    </div>`;
    $("#cmp-result").querySelectorAll("tr.click").forEach((tr) => tr.addEventListener("click", () => {
      const fv = $("#val-form");
      fv.vdays.value = validation.days;
      fv.window.value = validation.window_days;
      fv.step.value = validation.step_days;
      $("#bt-form").days.value = f.days.value;
      openBacktest(tr.dataset.id, symbols);
    }));
  } finally {
    state.comparing = false;
    $("#cmp-run").disabled = false;
    $("#cmp-progress").hidden = true;
  }
});

// ---------- stratégies ----------
function renderStrategies() {
  const families = [...new Set(state.presets.map((p) => p.family))];
  $("#strategy-list").innerHTML = families.map((f) => `<div class="family"><h2>${esc(f)}</h2>${state.presets.filter((p) => p.family === f).map((p) => `
    <div class="panel strategy"><div>
      <h3>${esc(p.name)} <span class="badge">${esc(p.timeframe)}</span></h3>
      <div class="desc">${esc(p.description)}</div>
      <div class="rules">Sorties : ${esc(exitsText(p))} · Taille : ${esc(sizingText(p))}</div>
    </div><div class="form-actions">
      <button class="ghost small" data-bt="${esc(p.id)}">Backtester</button>
      ${p.rule.type === "buy_hold" ? `<span class="muted small">incluse dans chaque portefeuille</span>` : `<button class="small" data-new="${esc(p.id)}">Suivre en direct</button>`}
    </div></div>`).join("")}</div>`).join("");
}
$("#strategy-list").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b) return;
  if (b.dataset.bt) openBacktest(b.dataset.bt);
  if (b.dataset.new) {
    show("portfolios");
    $("#new-portfolio").hidden = false;
    $("#np-preset").value = b.dataset.new;
    $("#np-preset").dispatchEvent(new Event("change"));
    $("#new-portfolio [name=name]").value = preset(b.dataset.new).name;
    $("#new-portfolio [name=cash]").value = state.settings.default_cash;
  }
});

// ---------- réglages ----------
function renderSettings() {
  const s = state.settings;
  const f = $("#settings-form");
  f.fee.value = +(s.costs.fee_rate * 100).toFixed(4);
  f.slip.value = s.costs.slippage_bps;
  f.cash.value = s.default_cash;
  f.oos.value = Math.round(s.oos_fraction * 100);
  f.tick.value = s.tick_seconds;
  $("#rt-cost").textContent = pct(2 * s.costs.fee_rate * 100 + (2 * s.costs.slippage_bps) / 100, 2).replace("+", "");
}
$("#settings-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const f = e.target;
  const settings = {
    costs: { fee_rate: Number(f.fee.value) / 100, slippage_bps: Number(f.slip.value) },
    default_cash: Number(f.cash.value),
    oos_fraction: Number(f.oos.value) / 100,
    tick_seconds: Number(f.tick.value),
  };
  state.settings = await call("save_settings", { settings });
  renderSettings();
  toast("Réglages enregistrés.");
});

function renderAutostart(s) {
  $("#autostart").checked = s.enabled;
  $("#autostart-info").innerHTML = s.enabled
    ? `À chaque ouverture de session, Windows lance <span class="mono">${esc(s.exe)}</span>, réduit dans la barre des tâches : les portefeuilles avancent sans que tu ouvres l'application.`
    : `Désactivé : les portefeuilles n'avancent que lorsque l'application est ouverte. Fermée, elle ne perd rien : à la réouverture, elle rejoue dans l'ordre les bougies clôturées entretemps.`;
}
$("#autostart").addEventListener("change", async (e) => {
  const want = e.target.checked;
  try {
    const s = await call("set_autostart", { enabled: want });
    renderAutostart(s);
    toast(s.enabled ? "PaperTrading se lancera au démarrage de Windows." : "Lancement au démarrage désactivé.");
  } catch {
    e.target.checked = !want;
  }
});

// ---------- barre de titre maison ----------
// Fermer passe par la même demande de fermeture que la croix de Windows : si le
// mode direct tourne, la fenêtre se range près de l'horloge au lieu de quitter.
const MAX_ICON = `<svg viewBox="0 0 10 10"><rect x=".5" y=".5" width="9" height="9" /></svg>`;
const RESTORE_ICON = `<svg viewBox="0 0 10 10"><rect x=".5" y="2.5" width="7" height="7" /><path d="M2.5 2.5V.5h7v7h-2" /></svg>`;
function setupWindowControls() {
  const win = TAURI.window.getCurrentWindow();
  $("#win-min").addEventListener("click", () => win.minimize());
  $("#win-max").addEventListener("click", () => win.toggleMaximize());
  $("#win-close").addEventListener("click", () => win.close());
  const syncMax = async () => {
    const max = await win.isMaximized();
    $("#win-max").innerHTML = max ? RESTORE_ICON : MAX_ICON;
    $("#win-max").title = max ? "Restaurer" : "Agrandir";
  };
  win.onResized(syncMax);
  syncMax();
}

// ---------- démarrage ----------
async function boot() {
  if (!TAURI) {
    document.body.innerHTML = `<main><div class="empty">Cette interface doit être ouverte dans l'application PaperTrading.</div></main>`;
    return;
  }
  setupWindowControls();
  state.presets = await call("catalog");
  state.settings = await call("get_settings");
  presetOptions($("#np-preset"), false);
  presetOptions($("#bt-preset"));
  $("#bt-preset").value = "macd_1d";
  $("#np-preset").dispatchEvent(new Event("change"));
  npPicker = symbolPicker($("#np-symbols"), DEFAULT_SYMBOLS.slice(0, 3));
  btPicker = symbolPicker($("#bt-symbols"), DEFAULT_SYMBOLS);
  cmpPicker = symbolPicker($("#cmp-symbols"), DEFAULT_SYMBOLS);
  renderStrategies();
  renderSettings();
  call("get_autostart").then(renderAutostart).catch(() => ($("#autostart").disabled = true));
  const info = await call("app_info");
  $("#app-info").innerHTML = `Version ${esc(info.version)} · données dans <span class="mono">${esc(info.data_dir)}</span> · bougies : <span class="mono">${esc(info.market_url)}</span>`;
  renderEngine(await call("engine_status"));
  await listen("engine-status", (e) => renderEngine(e.payload));
  await listen("portfolios-changed", () => {
    if (state.view === "portfolios") refreshPortfolios();
    if (state.view === "detail") refreshDetail();
  });
  await listen("comparison-progress", (e) => {
    const p = e.payload;
    $("#cmp-bar").max = p.total;
    $("#cmp-bar").value = p.done;
    $("#cmp-current").textContent = p.current ? `${p.done + 1}/${p.total} · ${p.current}` : "";
  });
  const start = new URLSearchParams(location.search).get("vue");
  show(start || "portfolios");
}
boot();

// ---------- un trade en détail : le prix de la crypto de l'achat à la vente ----------
async function showTrade(t) {
  const box = $("#trade-view");
  if (!box || !t) return;
  const tf = state.detail.summary.timeframe;
  box.innerHTML = `<div class="panel"><p class="muted">Chargement du prix de ${esc(t.symbol)}…</p></div>`;
  box.scrollIntoView({ behavior: "smooth", block: "start" });
  let pts;
  try { pts = await call("trade_candles", { symbol: t.symbol, timeframe: tf, from: t.entry_time, to: t.exit_time }); }
  catch (err) { box.innerHTML = `<div class="panel"><p class="neg">Prix indisponible : ${esc(String(err))}</p></div>`; return; }
  const fills = (state.detail.fills || []).filter((f) => f.symbol === t.symbol && f.time >= t.entry_time && f.time <= t.exit_time);
  const markers = (fills.length ? fills : [{ time: t.entry_time, price: t.avg_entry_price, side: "Buy" }, { time: t.exit_time, price: t.exit_price, side: "Sell" }])
    .map((f) => ({ time: f.time, value: f.price, side: f.side, label: `${f.side === "Buy" ? "Achat" : "Vente"} à ${price(f.price)} · ${dateTime(f.time)}` }));
  const why = whyTrade(t);
  const inTrade = pts.filter((p) => p[0] >= t.entry_time && p[0] <= t.exit_time);
  const hi = inTrade.length ? Math.max(...inTrade.map((p) => p[1])) : null, lo = inTrade.length ? Math.min(...inTrade.map((p) => p[1])) : null;
  const rel = (v) => (v == null ? "—" : signed(((v - t.avg_entry_price) / t.avg_entry_price) * 100, (x) => pct(x, 2)));
  box.innerHTML = `<div class="panel"><h2>${esc(t.symbol)} · ${signed(t.pnl, money)} (${signed(t.return_pct, (v) => pct(v, 2))})</h2>
    <div class="grid-5">
      <div class="kpi"><div class="v">${price(t.avg_entry_price)}</div><div class="l">acheté le ${dateTime(t.entry_time)}</div></div>
      <div class="kpi"><div class="v">${price(t.exit_price)}</div><div class="l">vendu le ${dateTime(t.exit_time)}</div></div>
      <div class="kpi"><div class="v pos">${rel(hi)}</div><div class="l">plus haut pendant le trade</div></div>
      <div class="kpi"><div class="v neg">${rel(lo)}</div><div class="l">plus bas pendant le trade</div></div>
      <div class="kpi"><div class="v">${esc(t.exit_reason)}</div><div class="l">${t.bars_held} bougie(s) de ${esc(tf)}</div></div>
    </div>
    ${lineChart([
      { name: `Prix de ${t.symbol} (clôtures ${tf})`, color: "#e3ba55", points: pts },
      { name: "Prix d'achat", color: "#7f8aa3", width: 1, points: [[pts[0]?.[0] ?? t.entry_time, t.avg_entry_price], [pts.at(-1)?.[0] ?? t.exit_time, t.avg_entry_price]] },
    ], { markers, yFormat: (v) => price(v) })}
    ${why}
    <p class="muted small">▲ achat · ▼ vente — survole un marqueur pour le prix exact. Clique un autre trade dans le tableau pour le voir.</p></div>`;
}

// ---------- journal des décisions : pourquoi un trade, et les signaux écartés ----------
async function loadDecisions(id) {
  state.decisions = null;
  let ds;
  try { ds = await call("portfolio_decisions", { id }); } catch (err) { ds = []; }
  if (state.detailId !== id) return;
  state.decisions = ds;
  state.skipFilter = "";
  renderSkipped();
}
function renderSkipped() {
  const panel = $("#skipped-panel"), box = $("#skipped-box");
  if (!panel || !state.decisions) return;
  const all = state.decisions.filter((d) => d.kind === "Skipped").reverse();
  panel.querySelector("summary").textContent = `Signaux écartés (${all.length})`;
  if (!all.length) { box.innerHTML = '<p class="muted">Aucun signal écarté enregistré (le journal démarre avec cette version).</p>'; return; }
  const reasons = {};
  all.forEach((d) => { reasons[d.reason] = (reasons[d.reason] || 0) + 1; });
  const rows = all.filter((d) => !state.skipFilter || d.reason === state.skipFilter).slice(0, 300);
  box.innerHTML = `<select id="skip-filter"><option value="">Toutes raisons</option>${Object.entries(reasons).sort((a, b) => b[1] - a[1]).map(([r, n]) => `<option value="${esc(r)}"${r === state.skipFilter ? " selected" : ""}>${esc(r)} (${n})</option>`).join("")}</select>
    <div class="table-wrap"><table><tr><th>Heure</th><th>Symbole</th><th>Raison</th><th class="num">Prix</th><th class="num">Force</th><th>Indicateurs</th></tr>
    ${rows.map((d) => `<tr><td>${dateTime(d.time)}</td><td>${esc(d.symbol)}</td><td>${esc(d.reason)}</td><td class="num">${price(d.price)}</td><td class="num">${d.indicators.strength != null ? num(d.indicators.strength, 2) : "—"}</td><td class="muted small">${indText(d.indicators)}</td></tr>`).join("")}</table></div>
    ${all.length > rows.length && !state.skipFilter ? `<p class="muted small">300 plus récents affichés.</p>` : ""}`;
}
document.addEventListener("change", (e) => { if (e.target.id === "skip-filter") { state.skipFilter = e.target.value; renderSkipped(); } });
function indText(ind) {
  return Object.entries(ind || {}).filter(([k]) => k !== "strength").map(([k, v]) => `${esc(k)} ${num(v, Math.abs(v) < 10 ? 4 : 2)}`).join(" · ");
}
function whyTrade(t) {
  const ds = state.decisions;
  if (!ds) return '<p class="muted small">Journal des décisions en cours de chargement…</p>';
  const pick = (kind, at) => ds.filter((d) => d.symbol === t.symbol && d.kind === kind && d.time <= at + 60000).sort((a, b) => b.time - a.time)[0];
  const ent = pick("Entry", t.entry_time), ex = pick("Exit", t.exit_time);
  const ok = (d) => d && Math.abs(d.time - (d === ent ? t.entry_time : t.exit_time)) < 3 * 86400000;
  if (!ok(ent) && !ok(ex)) return `<p class="muted small">Pas d'instantané pour ce trade : il date d'avant l'enregistrement des décisions.</p>`;
  const keys = [...new Set([...Object.keys(ok(ent) ? ent.indicators : {}), ...Object.keys(ok(ex) ? ex.indicators : {})])].sort();
  const cell = (d, k) => (ok(d) && d.indicators[k] != null ? num(d.indicators[k], Math.abs(d.indicators[k]) < 10 ? 4 : 2) : "—");
  return `<h3>Pourquoi ce trade</h3><div class="table-wrap"><table>
    <tr><th></th><th>À l'entrée</th><th>À la sortie</th></tr>
    <tr><td>Motif</td><td>${ok(ent) ? esc(ent.reason) : "—"}</td><td>${ok(ex) ? esc(ex.reason) : esc(t.exit_reason)}</td></tr>
    <tr><td>Décidé le</td><td>${ok(ent) ? dateTime(ent.time) : "—"}</td><td>${ok(ex) ? dateTime(ex.time) : "—"}</td></tr>
    ${keys.map((k) => `<tr><td>${esc(k)}</td><td class="num">${cell(ent, k)}</td><td class="num">${cell(ex, k)}</td></tr>`).join("")}
  </table></div>`;
}
