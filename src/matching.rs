//! Logique de correspondance, portée du plugin .NET (SeriesProvider,
//! SeasonProvider, EpisodeProvider). Code pur, sans appel à l'hôte : il est
//! testé nativement avec `cargo test`.

use crate::model::{Episode, Season, Serie};
use unicode_normalization::UnicodeNormalization;

/// `NormalizeTitle` du plugin d'origine : décomposition NFD, accents retirés,
/// minuscules, seuls lettres/chiffres/espaces conservés, espaces compactés.
pub fn normalize_title(title: &str) -> String {
    let kept: String = title
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Distance de Levenshtein sur les caractères (pas les octets).
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Score de similarité 0–100 du plugin d'origine.
fn similarity(a: &str, b: &str) -> i32 {
    let max = a.chars().count().max(b.chars().count());
    if max == 0 {
        return 0;
    }
    100 - (levenshtein(a, b) * 100 / max) as i32
}

/// Seuil de similarité d'une correspondance approchée. L'original acceptait
/// > 75 %, ce qui rattache des séries officielles à un Kaï : « Dragon Ball
/// Daima » → « Dragon Ball Yabai » (77 %). Les titres Fankai les plus proches
/// entre eux sont à 90 % (« Dragon Ball Z Yabai » / « Dragon Ball Yabai ») et
/// une correspondance exacte passe toujours avant.
const FUZZY_MIN_SCORE: i32 = 85;

/// Un titre sans son « (AAAA) » (« Hunter x Hunter Kaï (2011) »).
fn without_year(title: &str) -> String {
    match year_in_name(title) {
        Some(y) => title.replace(&format!("({y})"), ""),
        None => title.to_owned(),
    }
}

/// `GetSearchResults` : la série au titre identique (accents, casse et
/// ponctuation ignorés), sinon la plus proche au-delà de [`FUZZY_MIN_SCORE`],
/// avec +15 quand l'année correspond, comme l'original.
pub fn best_series<'a>(all: &'a [Serie], name: &str, year: Option<i32>) -> Option<&'a Serie> {
    let wanted = [normalize_title(name), normalize_title(&without_year(name))];
    if wanted[1].is_empty() {
        return None;
    }
    let mut best: Option<(&Serie, i32)> = None;
    for serie in all {
        let Some(title) = serie.title.as_deref() else { continue };
        let titles = [normalize_title(title), normalize_title(&without_year(title))];
        let mut score = wanted
            .iter()
            .flat_map(|w| titles.iter().map(move |t| similarity(w, t)))
            .max()
            .unwrap_or(0);
        if score < FUZZY_MIN_SCORE {
            continue;
        }
        // Le titre complet identique (année comprise) départage deux homonymes.
        if wanted[0] == titles[0] {
            score += 1;
        }
        if year.is_some() && year == serie.year {
            score += 15;
        }
        // Premier arrivé gagne à égalité, comme l'OrderByDescending (stable) d'origine.
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((serie, score));
        }
    }
    best.map(|(s, _)| s)
}

/// `NormalizeFilenameForComparison` : minuscules, [..] et (..) retirés,
/// marqueurs techniques (1080p, x264, vostfr…) retirés, séparateurs → espace.
pub fn normalize_filename(name: &str) -> String {
    const NOISE: &[&str] = &[
        "1080p", "720p", "480p", "multi", "x264", "x265", "h264", "h265", "hevc", "bdrip",
        "dvdrip", "webrip", "webdl", "vostfr", "vf", "truefrench", "aac", "dts", "ac3", "opus",
        "flac", "complete", "uncut", "bluray", "hddvd", "remux", "hdr", "sdr",
    ];
    let lower = name.to_lowercase();
    // Retire les blocs [..] et (..) (non imbriqués, comme la regex paresseuse d'origine).
    let mut stripped = String::with_capacity(lower.len());
    let mut close: Option<char> = None;
    for c in lower.chars() {
        match close {
            Some(end) if c == end => {
                close = None;
                stripped.push(' ');
            }
            Some(_) => {}
            None if c == '[' => close = Some(']'),
            None if c == '(' => close = Some(')'),
            None => stripped.push(c),
        }
    }
    // Un bloc jamais fermé est gardé tel quel, comme une regex qui ne correspond pas.
    if close.is_some() {
        stripped = lower.clone();
    }
    // Les marqueurs sont des mots entiers (\b) : on découpe sur tout ce qui n'est
    // pas un caractère de mot, puis on recolle sur les séparateurs d'origine.
    let spaced: String = stripped
        .chars()
        .map(|c| if c.is_whitespace() || matches!(c, '.' | '-' | '_' | '\'') { ' ' } else { c })
        .collect();
    spaced
        .split_whitespace()
        .filter(|w| !NOISE.contains(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Les composants d'un chemin (séparateurs `/` et `\`), sans vides.
pub fn components(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|c| !c.is_empty()).collect()
}

/// Le nom de fichier sans extension.
pub fn file_stem(file: &str) -> &str {
    match file.rfind('.') {
        Some(i) if i > 0 => &file[..i],
        _ => file,
    }
}

/// `ParseSeasonNumberFromPath` : « Saison 2 », « Season 02 », « S2 »… et
/// « Specials » / « Spéciaux » (saison 0, où Fan-Kai range films et spéciaux).
/// Ajout : un dossier de release qui nomme sa saison ou sa saga en clair
/// (« Dragon Ball Z Yabai - Saga 2 - Namek (04-08) [Mixouille] »).
pub fn parse_season_folder(name: &str) -> Option<i32> {
    strict_season_folder(name).or_else(|| {
        let norm = normalize_title(name);
        let words: Vec<&str> = norm.split_whitespace().collect();
        words.windows(2).find_map(|w| {
            let numeric = w[1].len() <= 3 && w[1].chars().all(|c| c.is_ascii_digit());
            (matches!(w[0], "saison" | "season" | "saga") && numeric).then(|| w[1].parse().ok())?
        })
    })
}

fn strict_season_folder(name: &str) -> Option<i32> {
    let n = name.trim().to_lowercase();
    if matches!(
        normalize_title(&n).as_str(),
        "specials" | "special" | "speciaux" | "special episodes" | "saison 0" | "season 0"
    ) {
        return Some(0);
    }
    let rest = ["saisons", "saison", "seasons", "season", "s"]
        .iter()
        .find_map(|p| n.strip_prefix(p))?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(['.', '_', '-']).unwrap_or(rest).trim_start();
    if rest.is_empty() || rest.len() > 3 || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// Le motif `SxxEyy` d'un nom de fichier (« Slam Dunk Kaï.S01E02.VOSTFR… »).
pub fn parse_sxxexx(name: &str) -> Option<(i32, i32)> {
    let chars: Vec<char> = name.chars().collect();
    for i in 0..chars.len() {
        if !matches!(chars[i], 's' | 'S') || (i > 0 && chars[i - 1].is_alphanumeric()) {
            continue;
        }
        let mut j = i + 1;
        let s_start = j;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j == s_start || j - s_start > 3 || j >= chars.len() || !matches!(chars[j], 'e' | 'E') {
            continue;
        }
        let e_start = j + 1;
        let mut k = e_start;
        while k < chars.len() && chars[k].is_ascii_digit() {
            k += 1;
        }
        if k == e_start || k - e_start > 4 {
            continue;
        }
        let season: String = chars[s_start..j].iter().collect();
        let episode: String = chars[e_start..k].iter().collect();
        return Some((season.parse().ok()?, episode.parse().ok()?));
    }
    None
}

/// Où se trouve un élément dans l'arborescence (`Série/Saison N/fichier.mkv`,
/// `Série/fichier.mkv`, ou `Série/<dossier de release>/fichier.mkv`).
#[derive(Debug, PartialEq, Eq)]
pub struct EpisodePath<'a> {
    /// Les dossiers qui peuvent être celui de la série, du plus probable au
    /// moins probable : le parent (sauf si c'est un dossier de saison), puis
    /// le grand-parent.
    pub series_folders: Vec<&'a str>,
    pub season_number: Option<i32>,
    pub file_stem: &'a str,
}

pub fn split_episode_path(path: &str) -> Option<EpisodePath<'_>> {
    let parts = components(path);
    let (file, dirs) = parts.split_last()?;
    let parent = dirs.last().copied();
    let grandparent = dirs.len().checked_sub(2).map(|i| dirs[i]);
    let season_number = parent.and_then(parse_season_folder);
    let series_folders = if strict_season_folder(parent.unwrap_or_default()).is_some() {
        grandparent.into_iter().collect()
    } else {
        parent.into_iter().chain(grandparent).collect()
    };
    Some(EpisodePath { series_folders, season_number, file_stem: file_stem(file) })
}

/// Le dossier de la série d'une saison (`…/Série/Saison 1` → `Série`).
pub fn series_folder_of_season(path: &str) -> Option<&str> {
    let parts = components(path);
    parts.len().checked_sub(2).map(|i| parts[i])
}

/// Le dossier d'une série (`…/Série` → `Série`).
pub fn last_component(path: &str) -> Option<&str> {
    components(path).last().copied()
}

/// Correspondance par nom de fichier (nom d'origine, nom du .nfo, nom
/// formaté Fan-Kai), en préférant un épisode numéroté, comme l'original.
pub fn match_episode_by_filename<'a>(
    seasons: &'a [(Season, Vec<Episode>)],
    stem: &str,
) -> Option<(&'a Season, &'a Episode)> {
    let wanted = normalize_filename(stem);
    if wanted.is_empty() {
        return None;
    }
    let same = |candidate: Option<&str>, strip_ext: bool| {
        candidate
            .map(|c| if strip_ext { file_stem(c) } else { c })
            .map(normalize_filename)
            .is_some_and(|c| !c.is_empty() && c == wanted)
    };
    let mut found: Vec<(&Season, &Episode)> = Vec::new();
    for (season, episodes) in seasons {
        for ep in episodes {
            if same(ep.original_filename.as_deref(), true)
                || same(ep.nfo_filename.as_deref(), true)
                || same(ep.formatted_name.as_deref(), false)
            {
                found.push((season, ep));
            }
        }
    }
    found
        .iter()
        .find(|(_, e)| e.episode_number.is_some())
        .or(found.first())
        .copied()
}

/// Correspondance par numéros (saison, épisode), d'abord `episode_number`
/// puis `display_episode`.
pub fn match_episode_by_number(
    seasons: &[(Season, Vec<Episode>)],
    season: i32,
    episode: i32,
) -> Option<(&Season, &Episode)> {
    let (s, eps) = seasons.iter().find(|(s, _)| s.season_number == Some(season))?;
    eps.iter()
        .find(|e| e.episode_number == Some(episode))
        .or_else(|| {
            eps.iter().find(|e| {
                e.display_episode.as_deref().and_then(|d| d.trim().parse().ok()) == Some(episode)
            })
        })
        .map(|e| (s, e))
}

/// Un texte de l'API nettoyé (l'API renvoie des `\r\r\r\n`) ; `None` si vide.
pub fn clean_text(text: Option<&str>) -> Option<String> {
    let cleaned: String = text?.chars().filter(|c| *c != '\r').collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

/// L'année d'une date `AAAA-MM-JJ`.
pub fn year_of(date: Option<&str>) -> Option<i32> {
    let d = date?.trim();
    d.get(..4).filter(|y| y.chars().all(|c| c.is_ascii_digit()))?.parse().ok()
}

/// Une année entre parenthèses dans un nom de dossier (« Série (1993) »).
pub fn year_in_name(name: &str) -> Option<i32> {
    let bytes = name.as_bytes();
    (0..bytes.len().saturating_sub(5)).find_map(|i| {
        let w = &name.get(i..i + 6)?;
        let inner = w.strip_prefix('(')?.strip_suffix(')')?;
        inner.chars().all(|c| c.is_ascii_digit()).then(|| inner.parse().ok())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serie(id: u64, title: &str, year: Option<i32>) -> Serie {
        Serie { id, title: Some(title.into()), year, ..Default::default() }
    }

    #[test]
    fn titres_normalises() {
        assert_eq!(normalize_title("Slam Dunk Kaï"), "slam dunk kai");
        assert_eq!(normalize_title("  Ao Ashi Henshū! "), "ao ashi henshu");
        assert_eq!(normalize_title("L'Arme   Secrète"), "larme secrete");
    }

    #[test]
    fn levenshtein_classique() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("kaï", "kai"), 1);
    }

    #[test]
    fn meilleure_serie() {
        let all = vec![
            serie(83, "Dragon Ball Yabai", Some(1986)),
            serie(29, "Dragon Ball Z Yabai", Some(1989)),
            serie(79, "Slam Dunk Kaï", Some(1993)),
        ];
        assert_eq!(best_series(&all, "Dragon Ball Yabai", None).map(|s| s.id), Some(83));
        assert_eq!(best_series(&all, "Dragon Ball Z Yabai", None).map(|s| s.id), Some(29));
        assert_eq!(best_series(&all, "Slam Dunk Kai", None).map(|s| s.id), Some(79));
        assert_eq!(best_series(&all, "Dragon Ball Daima", None).map(|s| s.id), None);
        assert_eq!(best_series(&all, "", None).map(|s| s.id), None);
    }

    #[test]
    fn series_officielles_non_rattachees() {
        let all = vec![
            serie(83, "Dragon Ball Yabai", Some(1986)),
            serie(5, "Dragon Ball Super Kaï", Some(2015)),
            serie(7, "One Piece Kaï", None),
        ];
        assert!(best_series(&all, "Dragon Ball Super Beerus", None).is_none());
        assert!(best_series(&all, "One Piece", None).is_none());
        assert!(best_series(&all, "Dragon Ball", None).is_none());
    }

    #[test]
    fn homonymes_par_annee() {
        let all = vec![
            serie(1, "Hunter x Hunter Kaï (1999)", Some(1999)),
            serie(2, "Hunter x Hunter Kaï (2011)", Some(2011)),
        ];
        assert_eq!(best_series(&all, "Hunter x Hunter Kaï (2011)", None).map(|s| s.id), Some(2));
        assert_eq!(best_series(&all, "Hunter x Hunter Kai", Some(2011)).map(|s| s.id), Some(2));
        assert_eq!(best_series(&all, "Hunter x Hunter Kaï (1999)", None).map(|s| s.id), Some(1));
    }

    #[test]
    fn noms_de_fichiers() {
        assert_eq!(
            normalize_filename("Slam Dunk Kaï.S01E01.VOSTFR.1080p.x264-FANKAI"),
            "slam dunk kaï s01e01 fankai"
        );
        assert_eq!(
            normalize_filename("[Titou] Slam dunk Kaï film 02 - L'arme secrète - VOSTFR X264 1080p"),
            "slam dunk kaï film 02 l arme secrète"
        );
    }

    #[test]
    fn dossiers_de_saison() {
        assert_eq!(parse_season_folder("Saison 1"), Some(1));
        assert_eq!(parse_season_folder("Season 02"), Some(2));
        assert_eq!(parse_season_folder("S3"), Some(3));
        assert_eq!(parse_season_folder("saison_4"), Some(4));
        assert_eq!(parse_season_folder("Specials"), Some(0));
        assert_eq!(parse_season_folder("Spéciaux"), Some(0));
        assert_eq!(parse_season_folder("Slam Dunk Kaï"), None);
        assert_eq!(parse_season_folder("Saison"), None);
        assert_eq!(parse_season_folder("Dragon Ball Z Yabai - Saga 4 - Majin Boo (15-22) [Mixouille]"), Some(4));
        assert_eq!(parse_season_folder("Les Saiyens (01-03)"), None);
    }

    #[test]
    fn sxxexx() {
        assert_eq!(parse_sxxexx("Slam Dunk Kaï.S01E07.VOSTFR"), Some((1, 7)));
        assert_eq!(parse_sxxexx("One Piece Yabai.s12e104"), Some((12, 104)));
        assert_eq!(parse_sxxexx("Dragon Ball Super Beerus"), None);
        assert_eq!(parse_sxxexx("Kais01e02"), None);
    }

    #[test]
    fn chemins() {
        let p = split_episode_path(
            "/media/animes/seagate/Slam Dunk Kaï/Saison 1/Slam Dunk Kaï.S01E02.VOSTFR.1080p.x264-FANKAI.mkv",
        )
        .unwrap();
        assert_eq!(p.series_folders, vec!["Slam Dunk Kaï"]);
        assert_eq!(p.season_number, Some(1));
        assert_eq!(p.file_stem, "Slam Dunk Kaï.S01E02.VOSTFR.1080p.x264-FANKAI");
        let flat = split_episode_path("/a/Serie/Serie.S01E01.mkv").unwrap();
        assert_eq!((flat.series_folders, flat.season_number), (vec!["Serie", "a"], None));
        let saga = split_episode_path(
            "/m/Dragon Ball Z Yabai/Dragon Ball Z Yabai - Saga 2 - Namek (04-08) [Mixouille]/x.mkv",
        )
        .unwrap();
        assert_eq!(saga.season_number, Some(2));
        assert_eq!(
            saga.series_folders,
            vec!["Dragon Ball Z Yabai - Saga 2 - Namek (04-08) [Mixouille]", "Dragon Ball Z Yabai"]
        );
        assert_eq!(series_folder_of_season("/a/Serie/Saison 1"), Some("Serie"));
        assert_eq!(last_component("/a/Serie/"), Some("Serie"));
    }

    #[test]
    fn divers() {
        assert_eq!(clean_text(Some("a\r\r\r\n\r\r\r\nb ")), Some("a\n\nb".into()));
        assert_eq!(clean_text(Some("  ")), None);
        assert_eq!(year_of(Some("2020-03-26")), Some(2020));
        assert_eq!(year_of(Some("")), None);
        assert_eq!(year_in_name("Slam Dunk (1993)"), Some(1993));
        assert_eq!(year_in_name("Slam Dunk"), None);
    }

    #[test]
    fn episodes_api_reelle() {
        // Extrait réel de /seasons/192/episodes (Slam Dunk Kaï).
        let json = include_str!("../tests/fixtures/slam_dunk_s1_episodes.json");
        let eps: crate::model::EpisodesResponse = serde_json::from_str(json).unwrap();
        let season = Season { id: 192, season_number: Some(1), ..Default::default() };
        let seasons = vec![(season, eps.episodes)];
        let (_, e) =
            match_episode_by_filename(&seasons, "Slam Dunk Kaï.S01E02.VOSTFR.1080p.x264-FANKAI")
                .unwrap();
        assert_eq!((e.id, e.title.as_deref()), (1188, Some("L'Arme Secrète")));
        let (_, e) = match_episode_by_filename(
            &seasons,
            "[Titou] Slam dunk Kaï film 01 - Un voyou au grand coeur - VOSTFR X264 1080p",
        )
        .unwrap();
        assert_eq!(e.id, 1186);
        let (_, e) = match_episode_by_number(&seasons, 1, 2).unwrap();
        assert_eq!(e.id, 1188);
        assert!(match_episode_by_filename(&seasons, "Autre chose").is_none());
    }
}
