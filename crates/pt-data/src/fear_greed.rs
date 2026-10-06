//! Indice Fear & Greed (alternative.me), historique complet, publié une fois par jour.

use anyhow::{Context, Result};
use serde_json::Value;

pub const URL: &str = "https://api.alternative.me/fng/?limit=0&format=json";

/// Rend (horodatage ms, valeur) trié par date croissante.
pub async fn fetch_fear_greed() -> Result<Vec<(i64, f64)>> {
    let body: Value = reqwest::Client::new()
        .get(URL)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .context("requête Fear & Greed")?
        .json()
        .await
        .context("réponse Fear & Greed illisible")?;
    parse(&body)
}

pub fn parse(body: &Value) -> Result<Vec<(i64, f64)>> {
    let data = body.get("data").and_then(|d| d.as_array()).context("champ data absent")?;
    let mut out: Vec<(i64, f64)> = data
        .iter()
        .filter_map(|d| {
            let ts = d.get("timestamp")?.as_str()?.parse::<i64>().ok()?;
            let v = d.get("value")?.as_str()?.parse::<f64>().ok()?;
            Some((ts * 1000, v))
        })
        .collect();
    out.sort_by_key(|x| x.0);
    out.dedup_by_key(|x| x.0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_and_sorts() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{"data":[{"value":"73","timestamp":"1791244800"},{"value":"70","timestamp":"1791158400"}]}"#,
        )
        .unwrap();
        let v = super::parse(&body).unwrap();
        assert_eq!(v, vec![(1791158400000, 70.0), (1791244800000, 73.0)]);
    }
}
