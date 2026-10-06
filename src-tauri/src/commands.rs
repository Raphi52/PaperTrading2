//! Commandes appelées par l'interface.

use crate::state::{AppState, EngineStatus, Settings};
use pt_core::backtest::{backtest, BacktestReport, CurvePoint, Metrics, Verdict};
use pt_core::portfolio::{ClosedTrade, Fill};
use pt_core::{catalog as all_presets, find, Engine, External, Preset, Timeframe};
use pt_data::HistoryCache;
use pt_store::{EquityRow, LivePortfolio};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;
type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn catalog() -> Vec<Preset> {
    all_presets()
}

#[tauri::command]
pub fn get_settings(state: St) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(state: St, settings: Settings) -> Res<Settings> {
    settings.validate()?;
    state.store.lock().expect("base").set_setting("settings", &settings).map_err(err)?;
    Ok(settings)
}

#[tauri::command]
pub fn engine_status(state: St) -> EngineStatus {
    state.status.lock().expect("statut").clone()
}

#[tauri::command]
pub fn set_engine_running(state: St, running: bool) -> Res<EngineStatus> {
    state.store.lock().expect("base").set_setting("engine_running", &running).map_err(err)?;
    let snapshot = {
        let mut st = state.status.lock().expect("statut");
        st.running = running;
        st.clone()
    };
    if running {
        state.wake.notify_one();
    }
    Ok(snapshot)
}

#[tauri::command]
pub fn tick_now(state: St) {
    state.wake.notify_one();
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioSummary {
    pub id: i64,
    pub name: String,
    pub preset_id: String,
    pub preset_name: String,
    pub family: String,
    pub timeframe: Timeframe,
    pub symbols: Vec<String>,
    pub created_at: i64,
    pub active: bool,
    pub last_error: Option<String>,
    pub initial_cash: f64,
    pub cash: f64,
    pub equity: f64,
    pub return_pct: f64,
    pub benchmark_equity: f64,
    pub benchmark_return_pct: f64,
    pub open_positions: usize,
    pub trades: usize,
    pub win_rate_pct: Option<f64>,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub fees_paid: f64,
    /// Message tant qu'aucune bougie n'a clôturé depuis la création.
    pub waiting: Option<String>,
}

fn summarize(p: &LivePortfolio) -> PortfolioSummary {
    let e = &p.engine;
    let pf = &e.portfolio;
    let equity = e.equity();
    let bench = p.benchmark.equity();
    let trades = pf.closed.len();
    let wins = pf.closed.iter().filter(|t| t.pnl > 0.0).count();
    let unrealized: f64 = pf
        .positions
        .values()
        .map(|pos| pos.qty * e.marks.get(&pos.symbol).copied().unwrap_or(pos.last_fill_price) - pos.cost)
        .sum();
    let started = e.states.values().any(|s| s.last_bar_open_time.is_some_and(|t| t >= e.trade_from));
    let waiting = (!started).then(|| {
        format!(
            "Démarre à la clôture de la première bougie {} ouverte après la création (aucun trade antidaté).",
            e.preset.timeframe
        )
    });
    PortfolioSummary {
        id: p.id,
        name: p.name.clone(),
        preset_id: e.preset.id.clone(),
        preset_name: e.preset.name.clone(),
        family: e.preset.family.clone(),
        timeframe: e.preset.timeframe,
        symbols: e.symbols.clone(),
        created_at: p.created_at,
        active: p.active,
        last_error: p.last_error.clone(),
        initial_cash: pf.initial_cash,
        cash: pf.cash,
        equity,
        return_pct: (equity / pf.initial_cash - 1.0) * 100.0,
        benchmark_equity: bench,
        benchmark_return_pct: (bench / p.benchmark.portfolio.initial_cash - 1.0) * 100.0,
        open_positions: pf.positions.len(),
        trades,
        win_rate_pct: (trades > 0).then(|| wins as f64 / trades as f64 * 100.0),
        realized_pnl: pf.closed.iter().map(|t| t.pnl).sum(),
        unrealized_pnl: unrealized,
        fees_paid: pf.fees_paid,
        waiting,
    }
}

#[tauri::command]
pub fn list_portfolios(state: St) -> Res<Vec<PortfolioSummary>> {
    let store = state.store.lock().expect("base");
    let mut out = Vec::new();
    for id in store.ids().map_err(err)? {
        match store.load(id) {
            Ok(p) => out.push(summarize(&p)),
            Err(e) => return Err(format!("{e:#}")),
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionView {
    pub symbol: String,
    pub qty: f64,
    pub avg_price: f64,
    pub mark: f64,
    pub value: f64,
    pub cost: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pct: f64,
    pub stop: Option<f64>,
    pub take_profit: Option<f64>,
    pub layers: u32,
    pub bars_held: u32,
    pub entry_time: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioDetail {
    pub summary: PortfolioSummary,
    pub preset: Preset,
    pub positions: Vec<PositionView>,
    pub trades: Vec<ClosedTrade>,
    pub fills: Vec<Fill>,
    pub curve: Vec<EquityRow>,
}

fn downsample<T: Copy>(v: &[T], max: usize) -> Vec<T> {
    if v.len() <= max || max < 2 {
        return v.to_vec();
    }
    let step = (v.len() - 1) as f64 / (max - 1) as f64;
    (0..max).map(|i| v[((i as f64) * step).round() as usize]).collect()
}

#[tauri::command]
pub fn portfolio_detail(state: St, id: i64) -> Res<PortfolioDetail> {
    let store = state.store.lock().expect("base");
    let p = store.load(id).map_err(|e| format!("{e:#}"))?;
    let e: &Engine = &p.engine;
    let positions = e
        .portfolio
        .positions
        .values()
        .map(|pos| {
            let mark = e.marks.get(&pos.symbol).copied().unwrap_or(pos.last_fill_price);
            let value = pos.qty * mark;
            PositionView {
                symbol: pos.symbol.clone(),
                qty: pos.qty,
                avg_price: pos.avg_price,
                mark,
                value,
                cost: pos.cost,
                unrealized_pnl: value - pos.cost,
                unrealized_pct: (value / pos.cost - 1.0) * 100.0,
                stop: pos.stop,
                take_profit: pos.take_profit,
                layers: pos.layers,
                bars_held: pos.bars_held,
                entry_time: pos.entry_time,
            }
        })
        .collect();
    let mut trades = e.portfolio.closed.clone();
    trades.reverse();
    let mut fills = e.portfolio.fills.clone();
    fills.reverse();
    let curve = downsample(&store.equity_curve(id).map_err(err)?, 1500);
    Ok(PortfolioDetail { summary: summarize(&p), preset: e.preset.clone(), positions, trades, fills, curve })
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRequest {
    pub name: String,
    pub preset_id: String,
    pub symbols: Vec<String>,
    pub cash: f64,
}

fn clean_symbols(raw: &[String]) -> Res<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for s in raw {
        let s = s.trim().to_uppercase();
        if s.is_empty() {
            continue;
        }
        if !s.chars().all(|c| c.is_ascii_alphanumeric()) || s.len() > 20 {
            return Err(format!("symbole invalide : {s}"));
        }
        if !out.contains(&s) {
            out.push(s);
        }
    }
    if out.is_empty() || out.len() > 20 {
        return Err("choisis entre 1 et 20 symboles".into());
    }
    out.sort();
    Ok(out)
}

#[tauri::command]
pub async fn create_portfolio(state: St<'_>, req: CreateRequest) -> Res<i64> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err("donne un nom au portefeuille (80 caractères au plus)".into());
    }
    let preset = find(&req.preset_id).ok_or_else(|| format!("stratégie inconnue : {}", req.preset_id))?;
    let symbols = clean_symbols(&req.symbols)?;
    if !(100.0..=1e9).contains(&req.cash) {
        return Err("le capital doit être entre 100 et 1 milliard".into());
    }
    for s in &symbols {
        state
            .client
            .recent(s, preset.timeframe, 5)
            .await
            .map_err(|e| format!("{s} introuvable sur Binance spot : {e:#}"))?;
    }
    let settings = state.settings();
    let now = pt_data::now_ms();
    let engine = Engine::new(preset, symbols.clone(), req.cash, settings.costs, now);
    let benchmark = Engine::new(find("buy_hold").expect("référence"), symbols, req.cash, settings.costs, now);
    let id = state.store.lock().expect("base").create(&name, now, &engine, &benchmark).map_err(err)?;
    state.wake.notify_one();
    Ok(id)
}

#[tauri::command]
pub fn set_portfolio_active(state: St, id: i64, active: bool) -> Res<()> {
    state.store.lock().expect("base").set_active(id, active).map_err(err)?;
    if active {
        state.wake.notify_one();
    }
    Ok(())
}

#[tauri::command]
pub fn delete_portfolio(state: St, id: i64) -> Res<()> {
    state.store.lock().expect("base").delete(id).map_err(err)
}

#[derive(Debug, Clone, Deserialize)]
pub struct BacktestRequest {
    pub preset_id: String,
    pub symbols: Vec<String>,
    pub days: i64,
    pub cash: f64,
}

async fn load(
    state: &AppState,
    preset: &Preset,
    symbols: &[String],
    days: i64,
) -> Res<(std::collections::BTreeMap<String, Vec<pt_core::Candle>>, External)> {
    if !(30..=3650).contains(&days) {
        return Err("la période doit être entre 30 jours et 10 ans".into());
    }
    let cache = HistoryCache::new(&state.cache_dir, state.client.clone());
    let start = pt_data::now_ms() - days * 86_400_000;
    let series = cache.series(symbols, preset.timeframe, start).await.map_err(|e| format!("{e:#}"))?;
    let mut ext = External::default();
    if preset.needs_fear_greed() {
        ext.fear_greed = state.fear_greed().await.map_err(|e| format!("{e:#}"))?;
    }
    Ok((series, ext))
}

#[tauri::command]
pub async fn run_backtest(state: St<'_>, req: BacktestRequest) -> Res<BacktestReport> {
    let preset = find(&req.preset_id).ok_or_else(|| format!("stratégie inconnue : {}", req.preset_id))?;
    let symbols = clean_symbols(&req.symbols)?;
    let settings = state.settings();
    let (series, ext) = load(&state, &preset, &symbols, req.days).await?;
    let mut r = backtest(&preset, &series, &ext, settings.costs, req.cash, settings.oos_fraction).map_err(err)?;
    r.curve = downsample::<CurvePoint>(&r.curve, 1500);
    r.benchmark_curve = downsample::<CurvePoint>(&r.benchmark_curve, 1500);
    Ok(r)
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompareRequest {
    pub symbols: Vec<String>,
    pub days: i64,
    pub cash: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompareRow {
    pub preset_id: String,
    pub preset_name: String,
    pub family: String,
    pub timeframe: Timeframe,
    pub start_time: i64,
    pub end_time: i64,
    pub split_time: i64,
    pub metrics: Metrics,
    pub benchmark: Metrics,
    pub oos: Metrics,
    pub benchmark_oos: Metrics,
    pub gross_return_pct: f64,
    pub fees_paid: f64,
    pub verdict: Verdict,
    pub verdict_label: String,
    pub verdict_reason: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Progress {
    done: usize,
    total: usize,
    current: String,
}

#[tauri::command]
pub async fn run_comparison(app: AppHandle, state: St<'_>, req: CompareRequest) -> Res<Vec<CompareRow>> {
    let symbols = clean_symbols(&req.symbols)?;
    let settings = state.settings();
    let presets = all_presets();
    let total = presets.len();
    let mut rows = Vec::new();
    for (i, p) in presets.iter().enumerate() {
        let _ = app.emit("comparison-progress", Progress { done: i, total, current: p.name.clone() });
        let res = match load(&state, p, &symbols, req.days).await {
            Ok((series, ext)) => {
                backtest(p, &series, &ext, settings.costs, req.cash, settings.oos_fraction).map_err(err)
            }
            Err(e) => Err(e),
        };
        rows.push(match res {
            Ok(r) => CompareRow {
                preset_id: r.preset_id,
                preset_name: r.preset_name,
                family: p.family.clone(),
                timeframe: r.timeframe,
                start_time: r.start_time,
                end_time: r.end_time,
                split_time: r.split_time,
                metrics: r.metrics,
                benchmark: r.benchmark,
                oos: r.oos,
                benchmark_oos: r.benchmark_oos,
                gross_return_pct: r.gross_return_pct,
                fees_paid: r.fees_paid,
                verdict_label: r.verdict.label().to_string(),
                verdict: r.verdict,
                verdict_reason: r.verdict_reason,
                error: None,
            },
            Err(e) => CompareRow {
                preset_id: p.id.clone(),
                preset_name: p.name.clone(),
                family: p.family.clone(),
                timeframe: p.timeframe,
                start_time: 0,
                end_time: 0,
                split_time: 0,
                metrics: Metrics::default(),
                benchmark: Metrics::default(),
                oos: Metrics::default(),
                benchmark_oos: Metrics::default(),
                gross_return_pct: 0.0,
                fees_paid: 0.0,
                verdict: Verdict::Insuffisant,
                verdict_label: "Erreur".into(),
                verdict_reason: e.clone(),
                error: Some(e),
            },
        });
    }
    let _ = app.emit("comparison-progress", Progress { done: total, total, current: String::new() });
    Ok(rows)
}

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub version: &'static str,
    pub data_dir: String,
    pub binance_url: String,
}

#[tauri::command]
pub fn app_info(state: St) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: state.data_dir.display().to_string(),
        binance_url: std::env::var("PT_BINANCE_URL").unwrap_or_else(|_| pt_data::binance::DEFAULT_BASE_URL.to_string()),
    }
}
