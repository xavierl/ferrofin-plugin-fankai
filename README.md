# Fankai pour Ferrofin

Portage pour [Ferrofin](https://github.com/mangoleaf/ferrofin) du plugin
[Fankai](https://github.com/Nackophilz/fankai_plugin) de Nackophilz, qui
récupère les métadonnées et les images des séries de la team Fan-Kai (Kaï,
Yabai, Henshū…) depuis l'API communautaire
[metadata.fankai.fr](https://metadata.fankai.fr).

Ferrofin ne charge pas les plugins .NET de Jellyfin. Ses plugins sont des
composants WebAssembly sandboxés (contrat `ferrofin:plugin@0.5.0`). Celui-ci est
une réécriture complète en Rust de la logique du plugin d'origine (v4.1.1).

> **English summary:** a Rust/WASM rewrite of the Fankai Jellyfin plugin for
> Ferrofin's `ferrofin:plugin@0.5.0` world. It matches series, seasons and
> episodes against metadata.fankai.fr and supplies overview, year, rating,
> genres, studio, Fankai ids and artwork. Titles, premiere dates, cast and theme
> music are not possible with the 0.5 contract (see below).

## Ce que fait le plugin

| | Séries | Saisons | Épisodes |
|---|---|---|---|
| Résumé | ✅ | ✅ | ✅ |
| Année | ✅ | ✅ | ✅ |
| Note, genres | ✅ | | |
| Studio, classification | ✅ | | ✅ |
| Identifiants Fankai¹ | `FankaiSerieId` | `FankaiSeasonId` | `FankaiEpisodeId` |
| Images² | affiche, fond | affiche, fond | vignette |

1. Ce sont les mêmes noms que dans le plugin .NET. Comme dans l'original depuis
   la v3.1.1, les identifiants TMDB, IMDb et TVDB de l'API ne sont **pas**
   repris : ils désignent l'anime d'origine, et TMDB rattacherait alors le Kaï
   à l'œuvre officielle.
2. Ferrofin ne demande aux plugins que l'affiche (Primary) et le fond
   (Backdrop), et seulement quand l'élément n'en a pas encore. Pour remplacer
   des images existantes, actualisez avec « Remplacer les images existantes ».

### Correspondance

Le fonctionnement est le même que dans le plugin d'origine :

- **Série** : le titre du dossier (ou de l'élément) est comparé à toutes les
  séries de l'API, sans tenir compte des accents, de la casse ni de la
  ponctuation. Un `FankaiSerieId` déjà posé est gardé tant qu'il correspond au
  dossier.
- **Épisode** : le nom du fichier est comparé au nom d'origine, au nom du
  `.nfo` et au nom normé Fan-Kai fournis par l'API. Les marqueurs techniques
  (1080p, x264, VOSTFR…) et les balises `[Team]` sont ignorés. À défaut, le
  plugin utilise le motif `SxxEyy`.
- **Saison** : numéro tiré du dossier (`Saison 1`, `Season 01`, `S1`,
  `Specials`).

Les différences avec l'original sont volontaires :

- **Correspondance approchée plus stricte.** Un titre identique passe
  toujours. Sinon, il faut au moins 85 % de similarité, contre 75 % dans
  l'original. À 75 %, des séries officielles étaient rattachées à un Kaï :
  « Dragon Ball Daima » à « Dragon Ball Yabai » (77 %).
- **Années entre parenthèses** (« Hunter x Hunter Kaï (2011) ») : elles servent
  à départager deux homonymes.
- **Dossiers de release** : un épisode rangé dans un sous-dossier comme
  `Dragon Ball Z Yabai - Saga 2 - Namek (04-08) [Mixouille]` est rattaché à la
  série du dossier parent, et « Saga N » donne le numéro de saison.

## Ce que le plugin ne peut pas faire (contrat Ferrofin 0.5)

Le contrat de plugin actuel de Ferrofin (`metadata-result`) n'a pas de champ
pour ces données. Elles restent donc hors de portée tant que le contrat
n'évolue pas :

- **les titres** : nom des épisodes (« L'Arme Secrète »), des saisons
  (« Tenkaichi Budokai ») et nom de tri. Les épisodes gardent le nom tiré du
  fichier ;
- la **date de sortie** (seule l'année passe) ;
- le **casting** ;
- le **`theme.mp3`** (le plugin n'a pas accès aux fichiers) ;
- l'**ordre absolu** (One Piece) ;
- les **bannières, logos et vignettes** de séries.

Le plugin les prendra en charge dès que le contrat de Ferrofin le permettra.

## Installation

### Depuis le dépôt de plugins (recommandé)

1. **Tableau de bord → Plugins → Dépôts** : ajoutez
   ```
   https://xavierl.github.io/ferrofin-plugin-fankai/manifest.json
   ```
2. **Catalogue** : installez **Fankai**, puis redémarrez Ferrofin.

### À la main

Téléchargez `ferrofin_plugin_fankai.wasm` depuis les
[releases](https://github.com/xavierl/ferrofin-plugin-fankai/releases), copiez-le
dans `{data_dir}/plugins/` (`/data/plugins/` dans le conteneur Docker), puis
redémarrez Ferrofin.

### Activation

1. **Tableau de bord → Bibliothèques**, ouvrez votre bibliothèque d'animés.
2. Pour les **Séries**, **Saisons** et **Épisodes**, cochez **Fankai** dans
   les récupérateurs de métadonnées et d'images (il est décoché par défaut),
   et placez-le **en tête**.
3. Actualisez les métadonnées de la bibliothèque, en remplaçant toutes les
   métadonnées.

Les séries que l'API ne connaît pas ne sont pas touchées : les fournisseurs
suivants (TheMovieDb…) s'en chargent comme avant.

La page de réglages du plugin accepte une clé d'API Fankai, facultative et
envoyée dans l'en-tête `X-API-Key`, comme dans l'original.

## Sécurité

Le plugin tourne dans la sandbox WASM de Ferrofin :

- il n'a accès à aucun fichier ;
- son seul accès réseau déclaré est `metadata.fankai.fr`
  (`egress` dans `Cargo.toml`), et le serveur refuse tout autre hôte ;
- il garde seulement, dans son état, la correspondance « élément → identifiants
  Fankai ». Elle sert à retrouver les images.

## Compilation

Il faut seulement [rustup](https://rustup.rs). La chaîne d'outils et la cible
`wasm32-wasip2` sont installées automatiquement.

```sh
cargo test        # tests de correspondance (natifs)
./build.sh        # → dist/ferrofin_plugin_fankai.wasm
```

Sans Rust installé :

```sh
docker run --rm -v "$PWD":/src -w /src rust:1-alpine \
  sh -c 'apk add -q musl-dev bash && cargo test && ./build.sh'
```

Pour publier une version, augmentez `version` dans `Cargo.toml` et poussez sur
`main`. La CI construit le `.wasm`, crée la release et republie `manifest.json`
sur GitHub Pages. Le projet part du
[modèle officiel](https://github.com/mangoleaf/ferrofin-plugin-template).

## Licence

MIT (voir [LICENSE](LICENSE)), comme le plugin d'origine. `wit/ferrofin-plugin.wit`
est l'interface de Ferrofin, sous GPL-3.0, et n'est inclus ici que pour la
compilation.

Merci à Nackophilz et à la communauté Fan-Kai pour le plugin d'origine et
l'API, et à mangoleaf pour Ferrofin.
