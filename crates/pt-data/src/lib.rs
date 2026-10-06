//! # pt-data
//!
//! Accès aux données PUBLIQUES de marché. Aucune clé d'API n'est nécessaire :
//! l'ancien projet stockait des clés en clair dans un fichier versionné.

pub mod binance;
pub mod cache;
pub mod fear_greed;

pub use binance::{BinanceClient, Recent};
pub use cache::HistoryCache;
pub use fear_greed::fetch_fear_greed;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
