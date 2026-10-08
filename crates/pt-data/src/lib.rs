//! # pt-data
//!
//! Accès aux données PUBLIQUES de marché. Aucune clé d'API n'est nécessaire :
//! l'ancien projet stockait des clés en clair dans un fichier versionné.
//!
//! Les prix viennent de Bitvavo, en paires EUR : celles qu'on peut réellement
//! acheter avec des euros depuis la France.

pub mod bitvavo;
pub mod cache;
pub mod fear_greed;
pub mod universe;

pub use bitvavo::{BitvavoClient, Recent};
pub use cache::HistoryCache;
pub use fear_greed::fetch_fear_greed;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
