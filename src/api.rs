//! Client de l'API Fankai, via `host::http_fetch` (le plugin n'a pas de
//! réseau à lui). Les réponses sont gardées en mémoire 30 minutes : un scan
//! interroge la même série pour chacun de ses épisodes.

use crate::bindings::ferrofin::plugin::host;
use crate::bindings::ferrofin::plugin::types::{HttpRequest, LogLevel};
use crate::model::{Episode, EpisodesResponse, Season, SeasonsResponse, Serie};
use serde::de::DeserializeOwned;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub const BASE_URL: &str = "https://metadata.fankai.fr";
const TTL_SECS: u64 = 30 * 60;
const MAX_ENTRIES: usize = 512;

thread_local! {
    /// chemin → (instant de lecture, corps ; `None` pour un 404).
    static CACHE: RefCell<HashMap<String, (u64, Option<String>)>> = RefCell::new(HashMap::new());
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn api_key() -> Option<String> {
    let config: serde_json::Value = serde_json::from_str(&host::get_config()).ok()?;
    config
        .get("FankaiApiKey")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
}

/// Le corps de `GET {BASE_URL}/{path}` ; `Ok(None)` pour un 404, `Err` pour
/// toute autre erreur (le scan redemandera alors l'élément au prochain passage).
fn fetch(path: &str) -> Result<Option<String>, String> {
    let t = now();
    if let Some(hit) = CACHE.with(|c| {
        c.borrow().get(path).filter(|(at, _)| t.saturating_sub(*at) < TTL_SECS).map(|(_, b)| b.clone())
    }) {
        return Ok(hit);
    }
    let mut headers = vec![
        ("Accept".to_owned(), "application/json".to_owned()),
        ("User-Agent".to_owned(), concat!("ferrofin-plugin-fankai/", env!("CARGO_PKG_VERSION")).to_owned()),
    ];
    if let Some(key) = api_key() {
        headers.push(("X-API-Key".to_owned(), key));
    }
    let url = format!("{BASE_URL}/{path}");
    let resp = host::http_fetch(&HttpRequest { method: "GET".to_owned(), url: url.clone(), headers, body: None })
        .map_err(|e| format!("API Fankai injoignable ({url}) : {e}"))?;
    let body = match resp.status {
        200..=299 => Some(String::from_utf8_lossy(&resp.body).into_owned()),
        404 => None,
        s => return Err(format!("API Fankai : HTTP {s} pour {url}")),
    };
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= MAX_ENTRIES {
            c.retain(|_, (at, _)| t.saturating_sub(*at) < TTL_SECS);
            if c.len() >= MAX_ENTRIES {
                c.clear();
            }
        }
        c.insert(path.to_owned(), (t, body.clone()));
    });
    Ok(body)
}

fn get<T: DeserializeOwned>(path: &str) -> Result<Option<T>, String> {
    match fetch(path)? {
        None => Ok(None),
        Some(body) => serde_json::from_str(&body)
            .map(Some)
            .map_err(|e| format!("réponse illisible de l'API Fankai pour /{path} : {e}")),
    }
}

pub fn all_series() -> Result<Vec<Serie>, String> {
    Ok(get("series?paginate=false")?.unwrap_or_default())
}

pub fn serie(id: &str) -> Result<Option<Serie>, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    get(&format!("series/{id}"))
}

pub fn seasons(serie_id: u64) -> Result<Vec<Season>, String> {
    Ok(get::<SeasonsResponse>(&format!("series/{serie_id}/seasons"))?.unwrap_or_default().seasons)
}

pub fn episodes(season_id: u64) -> Result<Vec<Episode>, String> {
    Ok(get::<EpisodesResponse>(&format!("seasons/{season_id}/episodes"))?.unwrap_or_default().episodes)
}

/// Toutes les saisons d'une série avec leurs épisodes.
pub fn seasons_with_episodes(serie_id: u64) -> Result<Vec<(Season, Vec<Episode>)>, String> {
    seasons(serie_id)?
        .into_iter()
        .map(|s| episodes(s.id).map(|eps| (s, eps)))
        .collect()
}

pub fn log(level: LogLevel, message: &str) {
    host::log(level, message);
}
