//! Les réponses de l'API Fankai (https://metadata.fankai.fr), réduites aux
//! champs dont le plugin se sert. Tous optionnels : l'API renvoie souvent
//! `null`, et un champ manquant ne doit jamais faire échouer la lecture.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Images {
    pub poster: Option<String>,
    pub fanart: Option<String>,
    pub banner: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Serie {
    pub id: u64,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub plot: Option<String>,
    pub premiered: Option<String>,
    pub genres: Option<String>,
    pub studio: Option<String>,
    pub rating_value: Option<f64>,
    pub mpaa: Option<String>,
    pub tagline: Option<String>,
    pub images: Option<Images>,
    pub poster_image: Option<String>,
    pub fanart_image: Option<String>,
    pub banner_image: Option<String>,
    pub logo_image: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Season {
    pub id: u64,
    pub season_number: Option<i32>,
    pub plot: Option<String>,
    pub premiered: Option<String>,
    pub images: Option<Images>,
    pub poster_image: Option<String>,
    pub fanart_image: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SeasonsResponse {
    #[serde(default)]
    pub seasons: Vec<Season>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Episode {
    pub id: u64,
    pub title: Option<String>,
    pub episode_number: Option<i32>,
    pub display_episode: Option<String>,
    pub plot: Option<String>,
    pub aired: Option<String>,
    pub mpaa: Option<String>,
    pub studio: Option<String>,
    pub original_filename: Option<String>,
    pub nfo_filename: Option<String>,
    pub formatted_name: Option<String>,
    pub thumb_image: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EpisodesResponse {
    #[serde(default)]
    pub episodes: Vec<Episode>,
}

impl Serie {
    pub fn poster(&self) -> Option<&str> {
        pick(self.images.as_ref().and_then(|i| i.poster.as_deref()), self.poster_image.as_deref())
    }
    pub fn fanart(&self) -> Option<&str> {
        pick(self.images.as_ref().and_then(|i| i.fanart.as_deref()), self.fanart_image.as_deref())
    }
    pub fn banner(&self) -> Option<&str> {
        pick(self.images.as_ref().and_then(|i| i.banner.as_deref()), self.banner_image.as_deref())
    }
    pub fn logo(&self) -> Option<&str> {
        pick(self.images.as_ref().and_then(|i| i.logo.as_deref()), self.logo_image.as_deref())
    }
}

impl Season {
    pub fn poster(&self) -> Option<&str> {
        pick(self.images.as_ref().and_then(|i| i.poster.as_deref()), self.poster_image.as_deref())
    }
    pub fn fanart(&self) -> Option<&str> {
        pick(self.images.as_ref().and_then(|i| i.fanart.as_deref()), self.fanart_image.as_deref())
    }
}

fn pick<'a>(a: Option<&'a str>, b: Option<&'a str>) -> Option<&'a str> {
    a.filter(|s| !s.trim().is_empty())
        .or(b.filter(|s| !s.trim().is_empty()))
}
