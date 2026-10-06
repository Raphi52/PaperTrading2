//! État partagé de l'application.

use pt_core::CostModel;
use pt_data::BinanceClient;
use pt_store::Store;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub costs: CostModel,
    pub default_cash: f64,
    /// Part finale de l'historique gardée hors échantillon dans les backtests.
    pub oos_fraction: f64,
    /// Intervalle entre deux passages du mode direct, en secondes.
    pub tick_seconds: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { costs: CostModel::default(), default_cash: 10_000.0, oos_fraction: 0.3, tick_seconds: 30 }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.costs.fee_rate) {
            return Err("les frais doivent être entre 0 et 100 %".into());
        }
        if !(0.0..=500.0).contains(&self.costs.slippage_bps) {
            return Err("le glissement doit être entre 0 et 500 points de base".into());
        }
        if !(100.0..=1e9).contains(&self.default_cash) {
            return Err("le capital par défaut doit être entre 100 et 1 milliard".into());
        }
        if !(0.1..=0.9).contains(&self.oos_fraction) {
            return Err("la part hors échantillon doit être entre 10 et 90 %".into());
        }
        if !(10..=3600).contains(&self.tick_seconds) {
            return Err("l'intervalle doit être entre 10 secondes et 1 heure".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EngineStatus {
    pub running: bool,
    pub busy: bool,
    pub last_tick: Option<i64>,
    pub last_duration_ms: Option<u64>,
    pub last_error: Option<String>,
    pub ticks: u64,
    pub bars_processed: u64,
    /// Une autre fenêtre de l'application fait déjà avancer les portefeuilles :
    /// celle-ci ne fait qu'afficher, et prendra le relais si l'autre se ferme.
    pub elsewhere: bool,
}

/// Historique Fear & Greed et instant de son téléchargement.
type FearGreedCache = Option<(i64, Vec<(i64, f64)>)>;

pub struct AppState {
    pub store: Mutex<Store>,
    pub status: Mutex<EngineStatus>,
    pub client: BinanceClient,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub fear_greed: Mutex<FearGreedCache>,
    pub wake: tokio::sync::Notify,
}

impl AppState {
    pub fn new(store: Store, data_dir: PathBuf) -> Self {
        let running = store.get_setting::<bool>("engine_running").ok().flatten().unwrap_or(true);
        AppState {
            store: Mutex::new(store),
            status: Mutex::new(EngineStatus { running, ..EngineStatus::default() }),
            client: BinanceClient::new(),
            cache_dir: data_dir.join("cache"),
            data_dir,
            fear_greed: Mutex::new(None),
            wake: tokio::sync::Notify::new(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.store.lock().expect("base").get_setting("settings").ok().flatten().unwrap_or_default()
    }

    /// Historique Fear & Greed, rafraîchi au plus une fois par heure.
    pub async fn fear_greed(&self) -> anyhow::Result<Vec<(i64, f64)>> {
        let now = pt_data::now_ms();
        if let Some((at, v)) = self.fear_greed.lock().expect("cache").clone() {
            if now - at < 3_600_000 {
                return Ok(v);
            }
        }
        let v = pt_data::fetch_fear_greed().await?;
        *self.fear_greed.lock().expect("cache") = Some((now, v.clone()));
        Ok(v)
    }
}
