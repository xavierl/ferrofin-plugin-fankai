//! Fankai pour Ferrofin.
//!
//! Portage WASM (contrat `ferrofin:plugin@0.5.0`) du plugin Jellyfin/Emby
//! Fankai de Nackophilz : https://github.com/Nackophilz/fankai_plugin.
//! Les métadonnées viennent de l'API communautaire https://metadata.fankai.fr.
//!
//! Ce que le contrat 0.5 permet, et donc ce que fait le plugin :
//! - séries, saisons, épisodes : résumé, année, note, genres, studio,
//!   classification et identifiants Fankai (`FankaiSerieId`,
//!   `FankaiSeasonId`, `FankaiEpisodeId`, les mêmes noms que le plugin .NET) ;
//! - images : affiche et fond (série, saison), vignette (épisode).
//!
//! Ce qu'il ne permet pas (voir le README) : titres de séries, saisons et
//! épisodes, date de sortie, casting, `theme.mp3`.
//!
//! Comme le plugin d'origine depuis sa v3.1.1, les identifiants TMDB/IMDb/TVDB
//! de l'API ne sont jamais renvoyés : ils désignent l'anime d'origine, et les
//! autres fournisseurs rattacheraient alors le Kaï à l'œuvre officielle.

#[allow(clippy::too_many_arguments)]
mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "plugin",
    });
}

mod api;
mod matching;
mod model;

use bindings::*;
use ferrofin::plugin::host;
use ferrofin::plugin::types::LogLevel;
use model::{Episode, Season, Serie};
use serde::{Deserialize, Serialize};

const SERIE_ID: &str = "FankaiSerieId";
const SEASON_ID: &str = "FankaiSeasonId";
const EPISODE_ID: &str = "FankaiEpisodeId";

struct Plugin;

/// Ce que le plugin a reconnu pour un élément, gardé dans l'état du plugin
/// pour que `remote-images` (qui ne reçoit pas les identifiants) le retrouve.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Resolved {
    serie: Option<u64>,
    season: Option<u64>,
    episode: Option<u64>,
}

fn state_key(item_id: &str) -> String {
    format!("item:{item_id}")
}

fn remember(item_id: &str, r: &Resolved) {
    if let Ok(bytes) = serde_json::to_vec(r) {
        // Best effort : un état plein n'empêche pas de répondre.
        let _ = host::set_state(&state_key(item_id), Some(&bytes));
    }
}

fn recall(item_id: &str) -> Vec<(String, String)> {
    let Some(r) = host::get_state(&state_key(item_id))
        .and_then(|b| serde_json::from_slice::<Resolved>(&b).ok())
    else {
        return Vec::new();
    };
    [(SERIE_ID, r.serie), (SEASON_ID, r.season), (EPISODE_ID, r.episode)]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k.to_owned(), v.to_string())))
        .collect()
}

fn id_of<'a>(ids: &'a [(String, String)], name: &str) -> Option<&'a str> {
    ids.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

/// Un nom de dossier sans son « (AAAA) » final, et cette année.
fn split_year(name: &str) -> (String, Option<i32>) {
    let year = matching::year_in_name(name);
    let clean = match year {
        Some(y) => name.replace(&format!("({y})"), ""),
        None => name.to_owned(),
    };
    (clean.trim().to_owned(), year)
}

/// Recherche une série par nom (puis par nom de dossier s'il diffère).
fn search_series(names: &[&str]) -> Result<Option<Serie>, String> {
    let all = api::all_series()?;
    let mut tried: Vec<String> = Vec::new();
    for raw in names {
        let year = split_year(raw).1;
        let norm = matching::normalize_title(raw);
        if norm.is_empty() || tried.contains(&norm) {
            continue;
        }
        tried.push(norm);
        if let Some(s) = matching::best_series(&all, raw, year) {
            return Ok(Some(s.clone()));
        }
    }
    Ok(None)
}

/// La série d'un élément. Un `FankaiSerieId` déjà connu est gardé tant que
/// son titre correspond au dossier ou au nom de l'élément ; sinon on
/// ré-identifie, comme le plugin d'origine.
fn resolve_series(known_id: Option<&str>, folder: Option<&str>, item_name: &str) -> Result<Option<Serie>, String> {
    if let Some(id) = known_id {
        if let Some(s) = api::serie(id)? {
            let api_title = matching::normalize_title(s.title.as_deref().unwrap_or_default());
            let fits = |n: &str| matching::normalize_title(&split_year(n).0) == api_title;
            if folder.is_some_and(fits) || fits(item_name) {
                return Ok(Some(s));
            }
            api::log(
                LogLevel::Debug,
                &format!(
                    "Fankai : l'ID {id} (« {} ») ne correspond pas à « {} » : ré-identification.",
                    s.title.as_deref().unwrap_or_default(),
                    folder.unwrap_or(item_name)
                ),
            );
        }
    }
    let mut names: Vec<&str> = Vec::new();
    names.push(item_name);
    if let Some(f) = folder {
        names.push(f);
    }
    search_series(&names)
}

struct SeasonMatch {
    serie: Serie,
    season: Season,
}

fn resolve_season(item: &ItemSummary, ids: &[(String, String)]) -> Result<Option<SeasonMatch>, String> {
    let path = item.path.as_deref().unwrap_or_default();
    let folder = matching::series_folder_of_season(path);
    let Some(serie) = resolve_series(id_of(ids, SERIE_ID), folder, folder.unwrap_or_default())? else {
        return Ok(None);
    };
    let number = matching::last_component(path)
        .and_then(matching::parse_season_folder)
        .or_else(|| matching::parse_season_folder(&item.name));
    let seasons = api::seasons(serie.id)?;
    let by_id = id_of(ids, SEASON_ID).and_then(|id| seasons.iter().find(|s| s.id.to_string() == id));
    // Un ID stocké qui désigne une autre saison que le dossier est ignoré.
    let by_id = by_id.filter(|s| number.is_none() || s.season_number == number);
    let season = by_id.or_else(|| number.and_then(|n| seasons.iter().find(|s| s.season_number == Some(n))));
    Ok(season.cloned().map(|season| SeasonMatch { serie, season }))
}

struct EpisodeMatch {
    serie: Serie,
    season: Season,
    episode: Episode,
}

fn resolve_episode(item: &ItemSummary, ids: &[(String, String)]) -> Result<Option<EpisodeMatch>, String> {
    let Some(path) = item.path.as_deref() else { return Ok(None) };
    let Some(parts) = matching::split_episode_path(path) else { return Ok(None) };
    let mut serie = None;
    for folder in &parts.series_folders {
        serie = resolve_series(id_of(ids, SERIE_ID), Some(folder), folder)?;
        if serie.is_some() {
            break;
        }
    }
    let Some(serie) = serie else { return Ok(None) };
    let all = api::seasons_with_episodes(serie.id)?;
    // 1. l'ID déjà posé sur l'épisode, 2. le nom de fichier, 3. SxxEyy.
    let found = id_of(ids, EPISODE_ID)
        .and_then(|id| {
            all.iter().find_map(|(s, eps)| eps.iter().find(|e| e.id.to_string() == id).map(|e| (s, e)))
        })
        .or_else(|| matching::match_episode_by_filename(&all, parts.file_stem))
        .or_else(|| {
            matching::parse_sxxexx(parts.file_stem)
                .and_then(|(s, e)| matching::match_episode_by_number(&all, s, e))
        });
    Ok(found.map(|(s, e)| EpisodeMatch { serie: serie.clone(), season: s.clone(), episode: e.clone() }))
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn studios(s: Option<&str>) -> Vec<String> {
    non_empty(s).into_iter().collect()
}

fn empty_result() -> MetadataResult {
    MetadataResult {
        overview: None,
        production_year: None,
        community_rating: None,
        genres: Vec::new(),
        provider_ids: Vec::new(),
        tagline: None,
        studios: Vec::new(),
        tags: Vec::new(),
        official_rating: None,
        end_date: None,
    }
}

fn series_result(s: &Serie) -> MetadataResult {
    MetadataResult {
        overview: matching::clean_text(s.plot.as_deref()),
        production_year: s.year.or_else(|| matching::year_of(s.premiered.as_deref())),
        community_rating: s.rating_value.filter(|r| *r > 0.0),
        genres: s
            .genres
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_owned)
            .collect(),
        provider_ids: vec![(SERIE_ID.to_owned(), s.id.to_string())],
        tagline: non_empty(s.tagline.as_deref()),
        studios: studios(s.studio.as_deref()),
        official_rating: non_empty(s.mpaa.as_deref()),
        ..empty_result()
    }
}

fn season_result(m: &SeasonMatch) -> MetadataResult {
    MetadataResult {
        overview: matching::clean_text(m.season.plot.as_deref()),
        production_year: matching::year_of(m.season.premiered.as_deref()),
        provider_ids: vec![(SEASON_ID.to_owned(), m.season.id.to_string())],
        ..empty_result()
    }
}

fn episode_result(m: &EpisodeMatch) -> MetadataResult {
    MetadataResult {
        overview: matching::clean_text(m.episode.plot.as_deref()),
        production_year: matching::year_of(m.episode.aired.as_deref()),
        provider_ids: vec![
            (EPISODE_ID.to_owned(), m.episode.id.to_string()),
            (SEASON_ID.to_owned(), m.season.id.to_string()),
        ],
        studios: studios(m.episode.studio.as_deref()),
        official_rating: non_empty(m.episode.mpaa.as_deref()),
        ..empty_result()
    }
}

fn image(kind: &str, url: Option<&str>) -> Option<ImageCandidate> {
    let url = url?.trim();
    // Seules les URL de l'API sont dans l'egress déclaré ; une autre serait refusée par l'hôte.
    url.starts_with(api::BASE_URL).then(|| ImageCandidate {
        kind: kind.to_owned(),
        url: url.to_owned(),
        width: None,
        height: None,
        language: None,
    })
}

fn describe(item: &ItemSummary) -> String {
    item.path.clone().unwrap_or_else(|| item.name.clone())
}

impl Guest for Plugin {
    fn descriptor() -> PluginDescriptor {
        PluginDescriptor {
            id: env!("FERROFIN_PLUGIN_GUID").to_owned(),
            name: env!("FERROFIN_PLUGIN_NAME").to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            description: env!("FERROFIN_PLUGIN_DESCRIPTION").to_owned(),
        }
    }

    fn default_config() -> String {
        r#"{"FankaiApiKey":""}"#.to_owned()
    }

    fn config_pages() -> Vec<ConfigPage> {
        vec![ConfigPage {
            name: "fankai-config".to_owned(),
            content: include_str!("config_page.html")
                .replace("{PLUGIN_ID}", env!("FERROFIN_PLUGIN_GUID"))
                .replace("{VERSION}", env!("CARGO_PKG_VERSION"))
                .into_bytes(),
            enable_in_main_menu: false,
        }]
    }

    fn declared_egress() -> Vec<String> {
        env!("FERROFIN_PLUGIN_EGRESS").split(',').filter(|e| !e.is_empty()).map(str::to_owned).collect()
    }

    fn provider_info() -> Option<ProviderDescriptor> {
        Some(ProviderDescriptor {
            name: "Fankai".to_owned(),
            supported_kinds: vec!["Series".to_owned(), "Season".to_owned(), "Episode".to_owned()],
        })
    }

    fn scan_targets() -> Vec<String> {
        Vec::new()
    }

    fn scan_media(_item: ItemSummary) -> Result<(), String> {
        Ok(())
    }

    fn web_transforms() -> Vec<WebTransform> {
        Vec::new()
    }

    fn handle_request(_request: PluginRequest) -> PluginResponse {
        PluginResponse {
            status: 404,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: br#"{"error":"not found"}"#.to_vec(),
        }
    }

    fn tasks() -> Vec<TaskDescriptor> {
        Vec::new()
    }

    fn run_task(task_id: String) -> Result<(), String> {
        Err(format!("tâche inconnue `{task_id}`"))
    }

    fn on_event(_event_name: String, _event_json: String) {}

    fn metadata_lookup(
        item: ItemSummary,
        provider_ids: Vec<(String, String)>,
    ) -> Result<Option<MetadataResult>, String> {
        match item.kind.as_str() {
            "Series" => {
                let folder = item.path.as_deref().and_then(matching::last_component);
                let Some(s) = resolve_series(id_of(&provider_ids, SERIE_ID), folder, &item.name)? else {
                    api::log(LogLevel::Debug, &format!("Fankai : aucune série pour {}", describe(&item)));
                    return Ok(None);
                };
                api::log(
                    LogLevel::Info,
                    &format!("Fankai : « {} » → série {} « {} »", item.name, s.id, s.title.as_deref().unwrap_or_default()),
                );
                remember(&item.id, &Resolved { serie: Some(s.id), ..Default::default() });
                Ok(Some(series_result(&s)))
            }
            "Season" => {
                let Some(m) = resolve_season(&item, &provider_ids)? else {
                    api::log(LogLevel::Debug, &format!("Fankai : aucune saison pour {}", describe(&item)));
                    return Ok(None);
                };
                remember(&item.id, &Resolved { serie: Some(m.serie.id), season: Some(m.season.id), episode: None });
                Ok(Some(season_result(&m)))
            }
            "Episode" => {
                let Some(m) = resolve_episode(&item, &provider_ids)? else {
                    api::log(LogLevel::Debug, &format!("Fankai : aucun épisode pour {}", describe(&item)));
                    return Ok(None);
                };
                api::log(
                    LogLevel::Debug,
                    &format!(
                        "Fankai : {} → épisode {} « {} »",
                        describe(&item),
                        m.episode.id,
                        m.episode.title.as_deref().unwrap_or_default()
                    ),
                );
                remember(
                    &item.id,
                    &Resolved { serie: Some(m.serie.id), season: Some(m.season.id), episode: Some(m.episode.id) },
                );
                Ok(Some(episode_result(&m)))
            }
            _ => Ok(None),
        }
    }

    fn remote_images(item: ItemSummary) -> Result<Vec<ImageCandidate>, String> {
        let ids = recall(&item.id);
        let candidates = match item.kind.as_str() {
            "Series" => {
                let folder = item.path.as_deref().and_then(matching::last_component);
                match resolve_series(id_of(&ids, SERIE_ID), folder, &item.name)? {
                    Some(s) => vec![
                        image("Primary", s.poster()),
                        image("Backdrop", s.fanart()),
                        image("Banner", s.banner()),
                        image("Logo", s.logo()),
                    ],
                    None => Vec::new(),
                }
            }
            "Season" => match resolve_season(&item, &ids)? {
                Some(m) => vec![image("Primary", m.season.poster()), image("Backdrop", m.season.fanart())],
                None => Vec::new(),
            },
            "Episode" => match resolve_episode(&item, &ids)? {
                Some(m) => vec![image("Primary", m.episode.thumb_image.as_deref())],
                None => Vec::new(),
            },
            _ => Vec::new(),
        };
        Ok(candidates.into_iter().flatten().collect())
    }
}

bindings::export!(Plugin with_types_in bindings);
