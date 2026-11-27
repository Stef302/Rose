//! Mise à jour Windows par Velopack (deltas binaires + rollback).
//!
//! Velopack REMPLACE l'installeur WiX / NSIS : il installe l'application dans
//! un dossier versionné (`current\` désigne un dossier `app-<version>\`) et laisse un
//! launcher (`Update.exe`) rejouer la mise à jour et revenir à la version
//! précédente si la nouvelle ne démarre pas.
//!
//! Ce module reproduit côté Rust le PROTOCOLE Velopack, sans dépendance
//! (aucun crate `velopack`) :
//!
//! - `is_update_launcher` : Velopack relance l'application avec des arguments
//!   internes (`--veloapp-updated`, `--veloapp-obsolete`) ;
//! - `run_update_launcher` : sort en succès après l'application du paquet
//!   (`updated`) ou après un rollback (`obsolete`) ;
//! - `check_and_update` : DÉCOUVRE les releases via le MANIFESTE DU SITE Rose
//!   (`decouvrir_versions_update`), compare les versions sémantiques et ne
//!   télécharge QUE si un paquet COMPLET (`Type: Full`, les deltas ne sont pas
//!   rejoués ici) est publié — SHA-256 vérifié, dépôt dans `packages\`. Repli
//!   sur l'API GitHub puis sur le manifeste local `releases.win.json` si le
//!   site est indisponible ;
//!   aucune version n'est codée en dur, aucun nouveau launcher n'est requis ;
//! - `appliquer_paquets` : quand AUCUN launcher Velopack n'est installé
//!   (`Rose-win-Setup.exe` téléchargé à la main), on applique nous-mêmes les
//!   paquets en attente : extraction ZIP du `.nupkg`, copie de `lib/net45\`
//!   dans `current\`, écriture du fichier `current`. La version précédente
//!   reste sur disque (`app-<ancienne>`), donc un rollback manuel est possible.
//!
//! Sous Linux et macOS, `rose update` garde la voie artefacts GitHub
//! (`docs/telechargements/` + SHA-256) : Velopack n'a pas d'installeur natif
//! Linux. La voie reste testable ici avec `ROSE_VELOPACK_ROOT` (racine de
//! l'installation), `ROSE_VELOPACK_URL` (manifeste `file://` ou HTTP) et
//! `ROSE_VELOPACK=1`.
//!
//! Licence MIT.


use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Lance `curl` SANS fenêtre de console.
///
/// ⚠ `curl.exe` est un programme console : appelé depuis une application
/// GUI (l'installeur), Windows lui alloue une console — une fenêtre noire
/// apparaît brièvement. `CREATE_NO_WINDOW` la supprime.
fn curl_sans_fenetre() -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new("curl");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Variable posée par le launcher Velopack : sa présence prouve que Rose
/// tourne sous une installation Velopack.
pub const VAR_LAUNCHER: &str = "VELOCABI";

/// Dossier racine posé par Velopack : `%LOCALAPPDATA%\Rose`.
pub const DOSSIER_VELOPACK: &str = "Rose";

/// Arguments internes du launcher.
pub const ARG_MIS_A_JOUR: &str = "--veloapp-updated";
pub const ARG_OBSOLETE: &str = "--veloapp-obsolete";

/// Vrai si le processus a été lancé par le launcher Velopack avec un argument
/// interne (mise à jour appliquée ou ancienne version devenue obsolète).
pub fn is_update_launcher(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .any(|a| a == ARG_MIS_A_JOUR || a == ARG_OBSOLETE)
}

/// Répond au launcher et rend le code de sortie.
///
/// - `--veloapp-updated` : la nouvelle version est posée ; le launcher a déjà
///   écrit `current\` et le PATH — on sort en succès.
/// - `--veloapp-obsolete` : Velopack a rejoué l'ancienne version après un
///   rollback ; on ne touche à rien et on sort en succès.
pub fn run_update_launcher(args: &[String]) -> i32 {
    let _ = args;
    0
}

/// Racine de l'installation Velopack (`%LOCALAPPDATA%\Rose`).
///
/// `ROSE_VELOPACK_ROOT` force la racine (tests et installations hors profil).
pub fn dossier_racine() -> Option<PathBuf> {
    if let Some(r) = std::env::var_os("ROSE_VELOPACK_ROOT") {
        return Some(PathBuf::from(r));
    }
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join(DOSSIER_VELOPACK))
}

/// Vrai si Rose tourne depuis une installation Velopack : le launcher pose
/// `VELOCABI` (ou `ROSE_VELOPACK=1` pour forcer la voie, tests compris).
pub fn est_velopack() -> bool {
    std::env::var_os(VAR_LAUNCHER).is_some() || std::env::var_os("ROSE_VELOPACK").is_some()
}

/// Version installée : contenu du fichier `current` (`app-<version>`), sinon le
/// dossier `app-*` le plus récent. `None` hors d'une installation Velopack.
pub fn version_installee() -> Option<String> {
    let racine = dossier_racine()?;
    let courant = racine.join("current");
    // `current` est un petit fichier texte qui nomme le dossier versionné.
    if let Ok(nom) = fs::read_to_string(&courant) {
        let nom = nom.trim();
        if let Some(v) = nom.strip_prefix("app-") {
            return Some(v.to_string());
        }
    }
    let mut versions: Vec<String> = fs::read_dir(&racine)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let nom = e.file_name().to_string_lossy().into_owned();
            nom.strip_prefix("app-").map(|v| v.to_string())
        })
        .collect();
    versions.sort_by(|a, b| comparer_versions(a, b));
    versions.pop()
}

/// Dossier réellement utilisé par l'installation : `current\` s'il existe,
/// sinon `app-<version la plus récente>`.
pub fn dossier_courant() -> Option<PathBuf> {
    let racine = dossier_racine()?;
    let courant = racine.join("current");
    if courant.is_dir() {
        return Some(courant);
    }
    version_installee().map(|v| racine.join(format!("app-{v}")))
}

/// Compare deux versions `x.y.z` (segments absents = 0).
///
/// Chaque segment est lu chiffre par chiffre : un suffixe non numérique
/// (« 4-beta ») ne peut PAS s'effondrer sur 0, ce qui ferait croire que
/// un suffixe non numérique ne peut pas s'effondrer sur 0.
pub fn comparer_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let seg = |s: &str| -> Vec<u64> {
        s.split(['.', '-', '+'])
            .map(|p| {
                let chiffres: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                chiffres.parse::<u64>().unwrap_or(0)
            })
            .collect()
    };
    let (va, vb) = (seg(a), seg(b));
    for i in 0..va.len().max(vb.len()) {
        let x = *va.get(i).unwrap_or(&0);
        let y = *vb.get(i).unwrap_or(&0);
        if x != y {
            return x.cmp(&y);
        }
    }
    std::cmp::Ordering::Equal
}

/// Entrée d'un paquet dans `releases.win.json`.
#[derive(Debug, Clone)]
pub struct Paquet {
    pub version: String,
    pub nom: String,
    pub taille: u64,
    pub sha256: String,
    pub sha1: String,
    /// Vrai pour le paquet complet, faux pour un delta.
    pub complet: bool,
}

/// Manifeste Velopack réduit aux paquets Windows.
#[derive(Debug, Clone)]
pub struct Manifeste {
    pub paquets: Vec<Paquet>,
}

/// Extrait la valeur d'une clé dans un objet JSON plat (sans parseur : le
/// manifeste Velopack est une suite d'objets `{ "Clé": "Valeur", … }`).
fn valeur(json: &str, cle: &str) -> Option<String> {
    let motif = format!("\"{}\"", cle);
    let i = json.find(&motif)?;
    let reste = &json[i + motif.len()..];
    let d = reste.find(':')?;
    let reste = reste[d + 1..].trim_start();
    if let Some(r) = reste.strip_prefix('"') {
        let fin = r.find('"')?;
        return Some(r[..fin].to_string());
    }
    let fin = reste.find([',', '}']).unwrap_or(reste.len());
    Some(reste[..fin].trim().to_string())
}

impl Manifeste {
    /// Analyse `releases.win.json` : chaque paquet porte `PackageId`,
    /// `Version`, `SHA256`, `SHA1`, `Size` ; `Type: "Full"` ou `"Delta"`.
    pub fn analyser(json: &str) -> Result<Manifeste, String> {
        let mut paquets = Vec::new();
        let mut profondeur = 0i32;
        let mut debut = None;
        for (i, c) in json.char_indices() {
            match c {
                '{' => {
                    if profondeur == 0 {
                        debut = Some(i);
                    }
                    profondeur += 1;
                }
                '}' => {
                    profondeur -= 1;
                    if profondeur == 0 {
                        if let Some(d) = debut {
                            let obj = &json[d..=i];
                            if let Some(nom) = valeur(obj, "PackageId") {
                                let complet = valeur(obj, "Type")
                                    .map(|t| !t.eq_ignore_ascii_case("delta"))
                                    .unwrap_or(true);
                                paquets.push(Paquet {
                                    version: valeur(obj, "Version").unwrap_or_default(),
                                    nom,
                                    taille: valeur(obj, "Size")
                                        .and_then(|s| s.parse::<u64>().ok())
                                        .unwrap_or(0),
                                    sha256: valeur(obj, "SHA256").unwrap_or_default(),
                                    sha1: valeur(obj, "SHA1").unwrap_or_default(),
                                    complet,
                                });
                            }
                        }
                        debut = None;
                    }
                }
                _ => {}
            }
        }
        if paquets.is_empty() {
            return Err("manifeste Velopack vide ou illisible".into());
        }
        Ok(Manifeste { paquets })
    }

    /// Paquet COMPLET de la version la plus récente strictement supérieure à
    /// `installee`. Les deltas ne sont pas rejoués : seul le paquet complet
    /// est téléchargé.
    pub fn paquet_a_jour(&self, installee: &str) -> Option<&Paquet> {
        self.paquets
            .iter()
            .filter(|p| p.complet)
            .filter(|p| comparer_versions(&p.version, installee) == std::cmp::Ordering::Greater)
            .max_by(|a, b| comparer_versions(&a.version, &b.version))
    }
}

/// Lit un manifeste depuis une URL HTTP(S) ou un fichier `file://` —
/// `curl.exe` est présent sur Windows 10/11, aucune dépendance Rust.
fn lire_source(url: &str) -> Result<String, String> {
    if let Some(chemin) = url.strip_prefix("file://") {
        return fs::read_to_string(chemin).map_err(|e| format!("lecture {chemin} : {e}"));
    }
    let sortie = curl_sans_fenetre()
        .args(["-sSL", "--fail", url])
        .output()
        .map_err(|e| format!("curl indisponible : {e}"))?;
    if !sortie.status.success() {
        return Err(format!("téléchargement de {url} impossible (code {})", sortie.status));
    }
    String::from_utf8(sortie.stdout).map_err(|e| format!("réponse non UTF-8 : {e}"))
}

/// URL du manifeste Velopack : `ROSE_VELOPACK_URL` sinon la dernière release.
pub fn url_manifeste() -> String {
    if let Ok(u) = std::env::var("ROSE_VELOPACK_URL") {
        return u;
    }
    "https://github.com/Stef302/Rose/releases/latest/download/releases.win.json".to_string()
}

/* ═══════════════════════════════════════════════════════════════════════
   MODE DE MISE À JOUR — CHOIX DU PREMIER ÉCRAN

   Le premier écran de l'installateur/launcher demande à l'utilisateur
   comment Rose doit gérer ses mises à jour. Le choix est persisté dans un
   JSON PLAT écrit sous le profil utilisateur (`%APPDATA%\Rose\config.json`
   sur Windows), relu à chaque démarrage.

   ⚠ Aucun crate externe : on réutilise `valeur()` (le parseur JSON maison
   déjà présent plus haut pour `releases.win.json`). Aucun service Windows,
   aucune tâche planifiée, aucune clé HKLM : le choix est un simple fichier
   HKCU, sans privilège administrateur. Voir `docs/AUTO_UPDATE_CHOIX.md`.
   ═══════════════════════════════════════════════════════════════════════ */

/// Mode choisi dans les Options du launcher, AVANT toute installation.
///
/// Les trois valeurs visibles dans l'UI sont :
///   * `Desactive`  → « Désactivée » (DÉFAUT) : aucun timer, aucun appel réseau ;
///   * `Auto`       → « À l'ouverture » : la banderole du premier écran (L5) ;
///   * `Automatique` → « Automatique (discret) » : UN timer Windows à 6 h.
///
/// `Manuel` est conservé comme valeur HISTORIQUE (configs déjà écrites) : il
/// n'est plus proposé dans l'UI, mais reste accepté et se comporte comme
/// « Désactivée » pour tout ce qui est automatique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeMiseAJour {
    /// « À l'ouverture » : vérifie les versions à chaque lancement et affiche
    /// la banderole ; ne télécharge qu'après accord explicite de l'utilisateur.
    Auto,
    /// Ancien mode « Manuel » (plus proposé) : aucune vérification automatique.
    Manuel,
    /// « Désactivée » (DÉFAUT) : aucun timer, aucun appel réseau automatique.
    Desactive,
    /// « Automatique (discret) » : UN seul timer Windows à 6 h, lancé uniquement
    /// dans ce mode ; un tick = une lecture du manifeste SITE (cache + 3 s),
    /// une pastille discrète si une version est disponible, puis plus rien.
    Automatique,
}

impl ModeMiseAJour {
    /// Valeur exacte écrite dans `config.json`.
    pub fn comme_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manuel => "manuel",
            Self::Desactive => "desactive",
            Self::Automatique => "automatique_discret",
        }
    }

    /// Accepte la valeur canonique et quelques alias lisibles. Inconnu →
    /// `None` : l'appelant retombe sur le mode par défaut.
    pub fn depuis_str(s: &str) -> Option<Self> {
        // `to_lowercase()` (Unicode) et non `to_ascii_lowercase()` : l'alias
        // « DÉSACTIVÉ » doit devenir « désactivé » (É → é), sinon il est
        // rejeté et retombe sur le défaut — mesuré en test.
        let normalise = s.trim().to_lowercase();
        match normalise.as_str() {
            // « automatique » seul reste un alias de « à l'ouverture » pour
            // ne pas réinterpréter les configs écrites avant la vague 72.
            "auto" | "automatique" | "automatic" | "startup" | "ouverture"
            | "a_l_ouverture" => Some(Self::Auto),
            "manuel" | "manual" => Some(Self::Manuel),
            "desactive" | "désactivé" | "desactivee" | "off" | "none" => Some(Self::Desactive),
            // Alias explicites du NOUVEAU mode discret. La valeur canonique
            // est `automatique_discret` : elle ne peut pas être confondue
            // avec l'ancien alias « automatique » (mode banderole).
            "automatique_discret" | "automatique (discret)" | "automatique-discret"
            | "auto_discret" | "auto-discret" | "discret" | "silencieux" => Some(Self::Automatique),
            _ => None,
        }
    }

    /// Vrai UNIQUEMENT pour « Automatique (discret) ». C'est ce garde-fou qui
    /// décide si le timer Windows (6 h) doit être posé : aucun autre mode ne
    /// lance quoi que ce soit, même pas un thread endormi.
    ///
    /// ⚠ Ce prédicat NE décide PAS de lancer le helper : « À l'ouverture »
    /// doit AUSSI lancer le helper et s'enregistrer dans `shell:startup`,
    /// mais SANS timer 6 h. Voir [`Self::doit_lancer_helper`].
    pub fn doit_demarrer_timer_discret(self) -> bool {
        self == Self::Automatique
    }

    /// Vrai pour les DEUX modes automatiques : « À l'ouverture » (`Auto`) et
    /// « Automatique (discret) » (`Automatique`). C'est ce garde-fou qui
    /// décide si le helper `rose-tray.exe` doit être lancé immédiatement
    /// (fin d'installation / « Continuer ») ET enregistré dans `shell:startup`
    /// pour le prochain logon.
    ///
    /// Seuls `Desactive` et le mode historique `Manuel` ne lancent RIEN.
    ///
    /// Différence avec [`Self::doit_demarrer_timer_discret`] : le timer 6 h
    /// reste réservé au mode discret. En mode `Auto`, le helper peut afficher
    /// son icône mais ne pose AUCUN timer et ne fait AUCUNE requête
    /// automatique — la vérification au démarrage et la banderole (L5)
    /// appartiennent au launcher (`doit_verifier_au_demarrage`).
    pub fn doit_lancer_helper(self) -> bool {
        matches!(self, Self::Auto | Self::Automatique)
    }

    /// Les trois seules valeurs proposées dans l'UI, dans l'ordre affiché.
    /// `Manuel` (historique) est ramené à `Desactive` pour le surlignage.
    pub fn options_ui() -> [(Self, &'static str, &'static str); 3] {
        [
            (Self::Desactive, "Désactivée",
             "Aucune vérification automatique : aucun timer, aucun appel réseau. C'est le choix par défaut."),
            (Self::Auto, "À l'ouverture",
             "Une seule lecture au lancement : la banderole affiche la version disponible. Aucun téléchargement sans votre accord."),
            (Self::Automatique, "Automatique (discret)",
             "Un seul timer Windows toutes les 6 heures, une requête au site Rose, une pastille discrète. Aucune fenêtre, aucun polling."),
        ]
    }

    /// Valeur à surligner dans l'UI (les 3 choix, pas le mode historique).
    pub fn pour_ui(self) -> Self {
        match self {
            Self::Manuel => Self::Desactive,
            autre => autre,
        }
    }
}

/// Mode par défaut : « Désactivée ». Le mode discret est NOUVEAU et n'est
/// JAMAIS activé sans action explicite de l'utilisateur. Voir UPDATE_OPT.md.
pub const MODE_PAR_DEFAUT: ModeMiseAJour = ModeMiseAJour::Desactive;

/// Nom du fichier de configuration.
pub const NOM_CONFIG: &str = "config.json";
/// Clé JSON du mode.
pub const CLE_MODE: &str = "mode";

/// Chemin de la configuration : `%APPDATA%\Rose\config.json` sur Windows.
///
/// `ROSE_CONFIG_DIR` le redirige (tests, profils nomades) ; sans APPDATA on
/// retombe sur la racine Velopack, puis sur un chemin relatif. Jamais HKLM.
pub fn chemin_config() -> PathBuf {
    if let Some(d) = std::env::var_os("ROSE_CONFIG_DIR") {
        return PathBuf::from(d).join(NOM_CONFIG);
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join(DOSSIER_VELOPACK).join(NOM_CONFIG);
    }
    if let Some(r) = dossier_racine() {
        return r.join(NOM_CONFIG);
    }
    PathBuf::from(NOM_CONFIG)
}

/// Configuration lue : pour l'instant un seul champ, le mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigMiseAJour {
    pub mode: ModeMiseAJour,
}

impl Default for ConfigMiseAJour {
    fn default() -> Self {
        Self { mode: MODE_PAR_DEFAUT }
    }
}

/// Analyse un JSON plat `{"mode":"auto"}`. Toute valeur absente, invalide
/// ou non-string retombe sur `MODE_PAR_DEFAUT` (jamais une erreur fatale).
pub fn parser_config(json: &str) -> ConfigMiseAJour {
    let mode = valeur(json, CLE_MODE)
        .and_then(|v| ModeMiseAJour::depuis_str(&v))
        .unwrap_or(MODE_PAR_DEFAUT);
    ConfigMiseAJour { mode }
}

/// Relit la configuration au démarrage. Fichier absent ou illisible =
/// `MODE_PAR_DEFAUT` (le premier écran permet de changer immédiatement).
pub fn lire_config() -> ConfigMiseAJour {
    match fs::read_to_string(chemin_config()) {
        Ok(s) => parser_config(&s),
        Err(_) => ConfigMiseAJour::default(),
    }
}

/// Écrit le choix de l'utilisateur. Crée le dossier au besoin.
pub fn ecrire_config(mode: ModeMiseAJour) -> Result<PathBuf, String> {
    let chemin = chemin_config();
    if let Some(parent) = chemin.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("création de {} : {e}", parent.display()))?;
    }
    let json = format!("{{\n  \"{}\": \"{}\"\n}}\n", CLE_MODE, mode.comme_str());
    fs::write(&chemin, json)
        .map_err(|e| format!("écriture de {} : {e}", chemin.display()))?;
    Ok(chemin)
}

/// Message renvoyé quand le mode « désactivé » bloque une opération.
fn message_desactive() -> String {
    format!(
        "mises à jour DÉSACTIVÉES par {} (mode \"desactive\") : aucune \
         vérification ni téléchargement, même depuis « rose update ». \
         Choisissez « À l'ouverture » ou « Automatique (discret) » dans les \
         Options du launcher pour réactiver.",
        chemin_config().display()
    )
}

/// Décision prise au démarrage : le launcher ne lance une vérification QUE
/// dans le mode « Auto ». « Manuel » et « Désactivé » ne font RIEN — pas
/// même un appel réseau.
pub fn doit_verifier_au_demarrage() -> bool {
    lire_config().mode == ModeMiseAJour::Auto
}

/// Vérification AU DÉMARRAGE, en mode « Auto » uniquement : DÉCOUVRE les
/// releases (GitHub, repli manifeste local) et renvoie la version disponible
/// SANS RIEN TÉLÉCHARGER.
/// Le téléchargement n'a lieu que si l'utilisateur accepte ensuite
/// (`check_and_update` depuis le bouton de consentement de l'UI).
pub fn verifier_au_demarrage() -> Result<Option<String>, String> {
    if !doit_verifier_au_demarrage() {
        return Ok(None); // manuel / désactivé : aucun appel, aucun réseau
    }
    verifier_manifeste()
}

/* ═══════════════════════════════════════════════════════════════════════
   DÉCOUVERTE DYNAMIQUE — PLUS DE MANIFESTE EMBARQUÉ FIGÉ

   Un launcher installé UNE fois doit voir TOUTES les releases suivantes
   sans qu'on lui réclaque un nouveau launcher. La source de vérité est
   donc le MANIFESTE DU SITE (`https://stef302.github.io/Rose/…
   /releases.json`), servi sans limite de quota — l'API GitHub n'est qu'un
   repli. Le `releases.win.json` figé dans l'installation (une seule version)
   arrive en dernier.

   Ordre exact, sans authentification :
     1. `ROSE_VELOPACK_URL` posé → CE manifeste (compatibilité, tests) ;
     2. `ROSE_LAUNCHER_API` posé → cette URL (diagnostic, tests `file://`) ;
     3. manifeste du SITE Rose (`lister_versions_site`, cache TTL 300 s) ;
     4. API GitHub publique (`lister_versions`) si le site échoue ;
     5. `<racine>/releases.win.json`, puis `url_manifeste()` (qui peut lui-
        même être un `file://`), en dernier recours.

   Aucune version n'est codée en dur : tout vient du JSON réellement reçu.
   Une release SANS `.nupkg` mais avec un installeur générique
   (`Rose-win-Setup.exe`) est bien ANNONCÉE (`installable: false`) : c'est
   précisément le cas des releases v0.1.77 à v0.1.79.
   ═══════════════════════════════════════════════════════════════════════ */

/// Versions d'un manifeste Velopack local, sous forme de releases distantes.
fn versions_depuis_manifeste(manifeste: &Manifeste) -> Vec<VersionDistante> {
    let mut versions: Vec<VersionDistante> = manifeste
        .paquets
        .iter()
        .filter(|p| p.complet)
        .map(|p| VersionDistante {
            tag: format!("v{}", p.version),
            version: p.version.clone(),
            titre: format!("Rose v{}", p.version),
            date: String::new(),
            nupkg: p.nom.clone(),
            installable: !p.sha256.is_empty(),
            legacy: String::new(),
        })
        .collect();
    versions.sort_by(|a, b| comparer_versions(&b.version, &a.version));
    versions.dedup_by(|a, b| a.version == b.version);
    versions
}

/// Repli HORS LIGNE : copie du manifeste dans la racine d'installation,
/// puis source configurée (`file://` ou HTTP). Jamais une liste inventée.
fn charger_manifeste_local() -> Result<Manifeste, String> {
    if let Some(racine) = dossier_racine() {
        if let Ok(texte) = fs::read_to_string(racine.join(NOM_CACHE)) {
            if let Ok(m) = Manifeste::analyser(&texte) {
                return Ok(m);
            }
        }
    }
    Manifeste::analyser(&lire_source(&url_manifeste())?)
}

/// Découvre les releases pour `rose update`.
///
/// Ordre RÉEL (le site est la source primaire, GitHub le simple repli) :
///   1. `ROSE_VELOPACK_URL` posé → CE manifeste (compatibilité, tests) ;
///   2. `ROSE_LAUNCHER_API` posé → cette URL (diagnostic, tests `file://`) ;
///   3. manifeste du SITE Rose (`ROSE_LAUNCHER_SITE_MANIFESTE` ou URL Pages,
///      cache frais d'abord, TTL 300 s) ;
///   4. API GitHub publique (`lister_versions`) si le site échoue ;
///   5. `<racine>/releases.win.json` puis `url_manifeste()` en dernier recours.
/// `Err` seulement si AUCUNE source n'est exploitable.
pub fn decouvrir_versions_update() -> Result<ResultatVersions, String> {
    if std::env::var_os("ROSE_VELOPACK_URL").is_some() {
        // Surcharge explicite (PRIORITAIRE) : on lit EXACTEMENT ce manifeste
        // (tests `file://`, diagnostics), sans aucun appel réseau.
        let manifeste = Manifeste::analyser(&lire_source(&url_manifeste())?)?;
        let versions = versions_depuis_manifeste(&manifeste);
        if versions.is_empty() {
            return Err("manifeste imposé : aucun paquet complet".into());
        }
        return Ok(ResultatVersions { versions, source: SourceVersions::Local });
    }
    if let Ok(url) = std::env::var("ROSE_LAUNCHER_API") {
        let json = requeter_json(&url)?;
        let versions = analyser_releases(&json);
        if versions.is_empty() {
            return Err("API de diagnostic : aucune release installable".into());
        }
        return Ok(ResultatVersions { versions, source: SourceVersions::GitHub });
    }
    let url_site = if site_source_ouverte() { Some(url_manifeste_site()) } else { None };
    decouvrir_versions_update_depuis(url_site.as_deref(), dossier_racine().as_deref(), false)
}

/// Cœur de la découverte, avec dépendances explicites (testable HORS LIGNE).
/// `url_site = None` = diagnostic GitHub forcé ; `force = true` = ignorer le
/// TTL du cache site (bouton « Rafraîchir », tests).
fn decouvrir_versions_update_depuis(
    url_site: Option<&str>,
    racine: Option<&Path>,
    force: bool,
) -> Result<ResultatVersions, String> {
    let site = match url_site {
        Some(url) => lister_versions_site_depuis(url, racine, force),
        None => Err("source site désactivée par une variable de diagnostic".into()),
    };
    match site {
        Ok(resultat) => Ok(resultat),
        Err(site_erreur) => match lister_versions() {
            Ok(versions) => Ok(ResultatVersions { versions, source: SourceVersions::GitHub }),
            Err(github) => {
                let local = charger_manifeste_local()
                    .map(|m| versions_depuis_manifeste(&m))
                    .map_err(|e| format!("manifeste local indisponible ({e})"));
                match local {
                    Ok(v) if !v.is_empty() => Ok(ResultatVersions {
                        versions: v,
                        source: SourceVersions::Local,
                    }),
                    Ok(_) => Err(format!(
                        "site indisponible ({site_erreur}) ; GitHub indisponible ({github}) ; manifeste local vide"
                    )),
                    Err(e) => Err(format!(
                        "site indisponible ({site_erreur}) ; GitHub indisponible ({github}) ; {e}"
                    )),
                }
            }
        },
    }
}

/// URLs du manifeste ET du paquet pour une version découverte.
///
/// Avec `ROSE_VELOPACK_URL` explicite on garde l'ancien comportement
/// (manifeste et paquet dans le MÊME dossier, typiquement un `file://` de
/// test). Sinon on lit le manifeste PROPRE AU TAG : celui de `/latest` ne
/// connaît qu'une seule version — c'est la cause du bug.
pub fn urls_paquet(tag: &str, nom: &str) -> (String, String) {
    if std::env::var_os("ROSE_VELOPACK_URL").is_some() {
        let manifeste = url_manifeste();
        let base = manifeste
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        (manifeste, format!("{base}/{nom}"))
    } else {
        (url_asset(tag, NOM_CACHE), url_asset(tag, nom))
    }
}

/// Dernière version annoncée, STRICTEMENT supérieure à `installee`, avec la
/// source RÉELLE. Aucun téléchargement : c'est la fonction de découverte
/// utilisée par `rose update` et ses tests.
pub fn decouvrir_derniere_version(
    installee: &str,
) -> Result<(Option<VersionDistante>, SourceVersions), String> {
    let resultat = decouvrir_versions_update()?;
    let source = resultat.source;
    let version = resultat
        .versions
        .into_iter()
        .filter(|v| comparer_versions(&v.version, installee) == std::cmp::Ordering::Greater)
        .max_by(|a, b| comparer_versions(&a.version, &b.version));
    Ok((version, source))
}

/// Version unique, pour les appelants qui n'ont pas besoin de la source.
pub fn derniere_version_disponible(installee: &str) -> Result<Option<VersionDistante>, String> {
    Ok(decouvrir_derniere_version(installee)?.0)
}

/// Vérification EXPLICITE (bouton « Vérifier les mises à jour », mode
/// manuel). Refusée en mode « désactivé », avec le message explicite.
pub fn verifier_manifeste() -> Result<Option<String>, String> {
    if lire_config().mode == ModeMiseAJour::Desactive {
        return Err(message_desactive());
    }
    // Lanceur NEUTRE : jamais sa propre version comme « version installée ».
    // Sans version détectée, on ne prétend PAS être à jour.
    let installee = version_installee().unwrap_or_default();
    Ok(derniere_version_disponible(&installee)?.map(|v| v.version))
}

/// SHA-256 d'un fichier (implémentation maison, sans dépendance).
fn sha256_fichier(chemin: &Path) -> Result<String, String> {
    let mut f = fs::File::open(chemin).map_err(|e| format!("ouverture : {e}"))?;
    let mut octets = Vec::new();
    f.read_to_end(&mut octets).map_err(|e| format!("lecture : {e}"))?;
    Ok(crate::sha256::sha256_hex(&octets))
}

/// DÉCOUVRE la dernière version via l'API GitHub (repli manifeste local),
/// télécharge SON paquet complet et le dépose dans `packages\`. Si la
/// release n'a pas de `.nupkg`, la version est annoncée mais RIEN n'est
/// téléchargé (voir [`decouvrir_versions_update`]).
///
/// ⚠ RESPECT DU CHOIX DU PREMIER ÉCRAN : en mode « désactivé », AUCUNE
/// vérification n'est faite — même quand cette fonction est appelée
/// explicitement par `rose update` — et un message explicite est renvoyé.
/// En mode « manuel », l'appel explicite est autorisé (c'est précisément
/// l'action de l'utilisateur). En mode « auto », cet appel est le
/// téléchargement consenti à la suite de `verifier_au_demarrage`.
///
/// `Ok(Some(version))` : paquet téléchargé et vérifié ;
/// `Ok(None)` : Rose est déjà à jour ; `Err` : tout autre échec.
pub fn check_and_update(silencieux: bool) -> Result<Option<String>, String> {
    if lire_config().mode == ModeMiseAJour::Desactive {
        return Err(message_desactive());
    }
    verifier_et_telecharger(silencieux)
}

/// Ancien corps de `check_and_update` : SUPPOSÉ explicite (consentement
/// déjà obtenu par l'appelant). Ne jamais appeler directement sans passer
/// par le garde-fou de mode ci-dessus.
fn verifier_et_telecharger(_silencieux: bool) -> Result<Option<String>, String> {
    // Lanceur NEUTRE : jamais sa propre version comme « version installée ».
    // Sans version détectée, on ne prétend PAS être à jour.
    let installee = version_installee().unwrap_or_default();
    let (cible, source) = match decouvrir_derniere_version(&installee)? {
        (Some(v), s) => (v, s),
        (None, _) => return Ok(None),
    };

    // Une release ANNONCÉE sans paquet complet ne peut PAS être appliquée par
    // le dossier `packages\`. On ne télécharge RIEN : jamais un `.exe` non
    // vérifié (aucun SHA-256 publié pour ces releases). Le message est
    // explicite et permet de migrer manuellement si on le souhaite — le
    // lanceur déjà installé, lui, reste parfaitement utilisable.
    if !cible.installable || cible.nupkg.is_empty() {
        return Err(format!(
            "Nouvelle version {} découverte via {} (release {}) : aucun paquet complet \
             .nupkg n'est publié pour cette version, aucun téléchargement effectué. \
             Le lanceur installé reste utilisable ; artefact proposé : {}.",
            cible.version,
            source.libelle(),
            cible.tag,
            if cible.legacy.is_empty() { "Rose-win-Setup.exe" } else { &cible.legacy }
        ));
    }

    // Le manifeste de `/latest` ne connaît qu'UNE version : on lit celui du
    // TAG découvert, puis on vérifie le SHA-256 du paquet complet.
    let (url_manifeste_tag, url_paquet) = urls_paquet(&cible.tag, &cible.nupkg);
    let json = lire_source(&url_manifeste_tag)
        .map_err(|e| format!("release {} : manifeste inaccessible ({e})", cible.tag))?;
    let manifeste = Manifeste::analyser(&json)?;
    let paquet = manifeste
        .paquets
        .iter()
        .find(|p| p.complet && comparer_versions(&p.version, &cible.version) == std::cmp::Ordering::Equal)
        .ok_or_else(|| {
            format!("release {} : aucun paquet complet pour {}", cible.tag, cible.version)
        })?
        .clone();

    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;
    let paquets = racine.join("packages");
    fs::create_dir_all(&paquets).map_err(|e| format!("création {} : {e}", paquets.display()))?;
    let cible_fichier = paquets.join(&paquet.nom);

    let statut = curl_sans_fenetre()
        .args(["-sSL", "--fail", "-o"])
        .arg(&cible_fichier)
        .arg(&url_paquet)
        .status()
        .map_err(|e| format!("curl indisponible : {e}"))?;
    if !statut.success() {
        return Err(format!("téléchargement du paquet {} impossible", paquet.nom));
    }

    // Sans SHA-256 annoncé, on REFUSE d'installer : mieux vaut un message
    // clair qu'un binaire non vérifié (voir docs/UPDATE_DYNAMIQUE.md).
    if paquet.sha256.is_empty() {
        let _ = fs::remove_file(&cible_fichier);
        return Err(format!("release {} : SHA-256 absent du manifeste, paquet refusé", cible.tag));
    }
    let h = sha256_fichier(&cible_fichier)?;
    if !h.eq_ignore_ascii_case(&paquet.sha256) {
        let _ = fs::remove_file(&cible_fichier);
        return Err(format!(
            "SHA-256 du paquet invalide (attendu {}, obtenu {})",
            paquet.sha256, h
        ));
    }
    Ok(Some(paquet.version))
}

/// Vrai si un launcher Velopack est installé dans la racine (fichier
/// `Update.exe` ou `sq.version` à côté de `current\`).
fn launcher_present(racine: &Path) -> bool {
    racine.join("Update.exe").is_file() || racine.join("sq.version").is_file()
}

/// Applique les paquets `.nupkg` présents dans `packages\` quand AUCUN
/// launcher Velopack n'est installé (installation faite à la main par
/// `Rose-win-Setup.exe`) : extraction ZIP du paquet, copie de `lib/net45\`
/// dans `app-<version>\`, puis écriture de `current`. La version précédente
/// reste sur disque : un rollback manuel consiste à réécrire `current`.
///
/// Rend `Ok(Some(version appliquée))`, `Ok(None)` s'il n'y a rien à faire
/// (launcher présent, ou aucun paquet en attente).
pub fn appliquer_paquets() -> Result<Option<String>, String> {
    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;
    if launcher_present(&racine) {
        // Le launcher Velopack fait le travail lui-même au prochain démarrage.
        return Ok(None);
    }
    let paquets = racine.join("packages");
    let mut nupkg: Option<PathBuf> = None;
    if let Ok(entrees) = fs::read_dir(&paquets) {
        for e in entrees.flatten() {
            let nom = e.file_name().to_string_lossy().into_owned();
            if nom.ends_with("-full.nupkg") {
                nupkg = Some(e.path());
            }
        }
    }
    let nupkg = match nupkg {
        Some(p) => p,
        None => return Ok(None),
    };

    // version lue dans `sq.version` du paquet, sinon dans le nom de fichier
    let version = lire_sq_version(&nupkg)
        .or_else(|| {
            nupkg.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .and_then(|s| s.strip_prefix("Rose-").map(|v| v.trim_end_matches("-full").to_string()))
        })
        .ok_or("version du paquet illisible")?;

    let destination = racine.join(format!("app-{version}"));
    fs::create_dir_all(&destination).map_err(|e| format!("création {} : {e}", destination.display()))?;
    extraire_net45(&nupkg, &destination)?;
    // `current` : fichier texte qui nomme le dossier versionné (format Velopack)
    fs::write(racine.join("current"), format!("app-{version}"))
        .map_err(|e| format!("écriture de current : {e}"))?;
    let _ = fs::remove_file(&nupkg);
    // Réaffirme l'association APRÈS la mise à jour, en pointant le chemin
    // STABLE `current\rose.exe` (pas `app-<version>\rose.exe`) : l'icône
    // et les verbes restent valides quand une nouvelle version est posée.
    // `appliquer_silencieuse` est un no-op hors Windows (tests Linux intact).
    let exe_courant = racine.join("current").join("rose.exe");
    if let Err(e) = crate::assoc_windows::appliquer_silencieuse(Some(&exe_courant)) {
        eprintln!("Association .rose après mise à jour : {e}");
    }
    Ok(Some(version))
}

/// Extrait `sq.version` d'un paquet (ZIP) et rend la version lue.
fn lire_sq_version(nupkg: &Path) -> Option<String> {
    let octets = fs::read(nupkg).ok()?;
    let donnees = entrees_zip(&octets)
        .into_iter()
        .find(|(n, _)| n == "sq.version")
        .map(|(_, d)| d)?;
    let xml = String::from_utf8_lossy(&donnees).into_owned();
    let i = xml.find("<version>")? + "<version>".len();
    let j = xml[i..].find("</version>")? + i;
    Some(xml[i..j].trim().to_string())
}

/// Extrait toutes les entrées `lib/net45/...` du paquet vers `destination`
/// (sous-dossiers recréés). Lecture du ZIP local uniquement : en-têtes
/// locaux + méthode « stored » (0) ou « deflate » (8).
fn extraire_net45(nupkg: &Path, destination: &Path) -> Result<(), String> {
    let octets = fs::read(nupkg).map_err(|e| format!("lecture du paquet : {e}"))?;
    let mut extraits = 0;
    for (nom, donnees) in entrees_zip(&octets) {
        let Some(relatif) = nom.strip_prefix("lib/net45/") else { continue };
        if relatif.is_empty() || relatif.ends_with('/') {
            continue;
        }
        let cible = destination.join(relatif.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = cible.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("création {} : {e}", parent.display()))?;
        }
        fs::write(&cible, &donnees).map_err(|e| format!("écriture {} : {e}", cible.display()))?;
        extraits += 1;
    }
    if extraits == 0 {
        return Err("paquet sans entrée lib/net45 (format inattendu)".into());
    }
    Ok(())
}


/// Parcourt les en-têtes locaux d'une archive ZIP et rend (nom, données).
fn entrees_zip(octets: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut sortie = Vec::new();
    let mut i = 0usize;
    while i + 30 <= octets.len() {
        if &octets[i..i + 4] != b"PK\x03\x04" {
            break;
        }
        let methode = u16::from_le_bytes([octets[i + 8], octets[i + 9]]);
        let taille_comp = u32::from_le_bytes([octets[i + 18], octets[i + 19], octets[i + 20], octets[i + 21]]) as usize;
        let taille_nom = u16::from_le_bytes([octets[i + 26], octets[i + 27]]) as usize;
        let taille_ext = u16::from_le_bytes([octets[i + 28], octets[i + 29]]) as usize;
        let debut_nom = i + 30;
        let debut_donnees = debut_nom + taille_nom + taille_ext;
        if debut_nom + taille_nom > octets.len() {
            break;
        }
        let nom = String::from_utf8_lossy(&octets[debut_nom..debut_nom + taille_nom]).into_owned();
        // Certains écrivains (flux ZIP non cherchable) laissent les tailles à 0
        // dans l'en-tête local : on prend alors la taille de l'entrée centrale.
        let mut taille = taille_comp;
        if taille == 0 {
            if let Some(t) = taille_centrale(octets, &nom) {
                taille = t;
            }
        }
        if debut_donnees + taille > octets.len() {
            break;
        }
        let brut = &octets[debut_donnees..debut_donnees + taille];
        let donnees = match methode {
            0 => brut.to_vec(),
            8 => crate::inflate::decompresser(brut).unwrap_or_default(),
            _ => Vec::new(),
        };
        sortie.push((nom, donnees));
        i = debut_donnees + taille;
    }
    sortie
}

/// Taille compressée d'une entrée, lue dans le répertoire central (EOCD →
/// en-têtes centraux `PK\x01\x02`).
fn taille_centrale(octets: &[u8], nom: &str) -> Option<usize> {
    // EOCD : signature PK\x05\x06, dernière occurrence
    let eocd = (0..octets.len().saturating_sub(4))
        .rev()
        .find(|&k| &octets[k..k + 4] == b"PK\x05\x06")?;
    let nb = u16::from_le_bytes([octets[eocd + 10], octets[eocd + 11]]) as usize;
    let mut i = u32::from_le_bytes([
        octets[eocd + 16],
        octets[eocd + 17],
        octets[eocd + 18],
        octets[eocd + 19],
    ]) as usize;
    for _ in 0..nb {
        if i + 46 > octets.len() || &octets[i..i + 4] != b"PK\x01\x02" {
            return None;
        }
        let taille = u32::from_le_bytes([octets[i + 20], octets[i + 21], octets[i + 22], octets[i + 23]]) as usize;
        let l_nom = u16::from_le_bytes([octets[i + 28], octets[i + 29]]) as usize;
        let l_ext = u16::from_le_bytes([octets[i + 30], octets[i + 31]]) as usize;
        let l_com = u16::from_le_bytes([octets[i + 32], octets[i + 33]]) as usize;
        let nom_i = String::from_utf8_lossy(&octets[i + 46..i + 46 + l_nom]).into_owned();
        if nom_i == nom {
            return Some(taille);
        }
        i += 46 + l_nom + l_ext + l_com;
    }
    None
}

/* ═══════════════════════════════════════════════════════════════════════
   LAUNCHER UNIQUE — un seul exécutable, les versions viennent du dépôt

   Principe (remplace « un Rose-win-Setup-<version>.exe par release ») :

   1. `lister_versions()` interroge l'API PUBLIQUE des releases GitHub
      (`GET /repos/Stef302/Rose/releases`). Aucune version n'est codée en
      dur : une release future apparaît dès qu'elle est publiée.
   2. L'utilisateur choisit un tag parmi ceux renvoyés par l'API.
   3. `telecharger_version(tag)` ne télécharge QUE le paquet complet
      (`Rose-<v>-full.nupkg`) de CE tag, vérifie son SHA-256, l'extrait dans
      `app-<v>\` et fait pointer `current\`. Le moteur Rose n'est PAS
      embarqué dans le launcher : il n'y a qu'UN téléchargement, une seule
      fois, pour la version choisie.
   4. `selectionner_version(tag)` rebascule `current\` sur une version déjà
      présente sur disque (changement de version sans rien retélécharger).
   5. Le binaire launcher reste IDENTIQUE d'une release à l'autre.

   Accès réseau : `curl` uniquement (présent sur Windows 10/11), borné à
   3 secondes, avec cache local `versions-cache.json` lu immédiatement par
   l'UI et rafraîchi en tâche de fond après 5 minutes. Aucun crate externe.
   `ROSE_LAUNCHER_API`, `ROSE_LAUNCHER_BASE`, `ROSE_LAUNCHER_REPO` permettent
   de tout tester hors ligne (voir les tests en fin de module).
   ═══════════════════════════════════════════════════════════════════════ */

/// Dépôt GitHub des releases (lecture seule — aucune publication ici).
pub const DEPOT: &str = "Stef302/Rose";

/// API qui liste TOUTES les releases (contrairement à `/releases/latest`,
/// qui ne donne que la dernière, et au manifeste `releases.win.json`, qui
/// ne contient qu'UNE version).
pub const API_RELEASES: &str =
    "https://api.github.com/repos/Stef302/Rose/releases?per_page=100";

/// Base des artefacts téléchargeables : `<base>/<tag>/<fichier>`.
pub const BASE_ARTIFACTS: &str = "https://github.com/Stef302/Rose/releases/download";

/// Nom du cache local du manifeste (dernière version consultée).
pub const NOM_CACHE: &str = "releases.win.json";

/// Cache local de la LISTE des versions : réponse brute de l'API GitHub
/// déjà récupérée, avec son horodatage. Aucune version n'y est inventée.
pub const NOM_CACHE_VERSIONS: &str = "versions-cache.json";

/// Durée de fraîcheur du cache de la liste (secondes). Au-delà, l'UI affiche
/// la liste du cache IMMÉDIATEMENT puis la rafraîchit en tâche de fond.
pub const TTL_CACHE_VERSIONS: u64 = 300;

/// Timeout maximum d'un appel réseau pour la liste des versions (ms).
/// Le téléchargement d'une version choisie garde son propre timeout curl.
pub const DELAI_RESEAU_MS: u64 = 3000;

/// Intervalle du timer Windows du mode « Automatique (discret) » : 6 heures.
/// Un seul `SetTimer` est posé, uniquement dans ce mode (voir UPDATE_OPT.md).
/// 21 600 000 ms tiennent dans le `u32` de `SetTimer` (max 4 294 967 295).
pub const INTERVALLE_TIMER_DISCRET_MS: u32 = 6 * 60 * 60 * 1000;

/// Un tick du timer discret ne doit JAMAIS faire plus d'UNE requête HTTP.
/// Le repli GitHub reste disponible pour les actions explicites de l'UI, pas
/// pour ce timer : la source de vérité est le manifeste du SITE.
pub const REPLI_GITHUB_DANS_TIMER_DISCRET: bool = false;

/// Vrai si un intervalle `SetTimer` respecte la règle « ≥ 6 heures » et tient
/// dans un `u32`. Fonction PURE : testable sur Linux, sans Windows.
pub fn intervalle_timer_discret_valide(ms: u32) -> bool {
    ms >= 6 * 60 * 60 * 1000
}

/// Décision PURE : faut-il afficher une pastille pour ce résultat ?
/// `true` seulement si une version RÉELLE, plus récente, a été annoncée.
pub fn doit_notifier_discret(bandeau: Option<&BandeauMiseAJour>) -> bool {
    bandeau.is_some()
}

/* ── SOURCE DE VÉRITÉ : LE SITE ROSE (repli : API GitHub) ──────────────────
   L'UI ne dépend PAS de l'API GitHub : elle lit UN manifeste publié sur le
   site, au même endroit que les humains. Une seule requête à l'ouverture du
   launcher, bornée à 3 s, mise en cache avec le MÊME TTL (300 s) et le même
   mécanisme de fichier horodaté que `versions-cache.json`. Aucun thread qui
   boucle, aucun timer, aucune icône cachée : la vérification n'a lieu que
   quand l'utilisateur ouvre le launcher (ou clique explicitement).
   ───────────────────────────────────────────────────────────────────────── */

/// Manifeste public des releases, publié sur le SITE (source de vérité).
/// Format attendu : liste de releases `{version, tag, date, titre,
/// assets:[{nom,url,sha256}], changelog}`, éventuellement enveloppée dans
/// `{"releases":[…]}`. Aucune version n'est codée en dur ici.
pub const URL_MANIFESTE_SITE: &str =
    "https://stef302.github.io/Rose/telechargements/releases.json";

/// Surcharge du manifeste site (diagnostic / tests, accepte `file://`).
pub const VAR_MANIFESTE_SITE: &str = "ROSE_LAUNCHER_SITE_MANIFESTE";

/// Cache local de la réponse BRUTE du manifeste site (horodatage inclus).
/// Même format et même TTL que `NOM_CACHE_VERSIONS` : on ne réinvente aucune
/// version, on relit exactement ce que le site a envoyé.
pub const NOM_CACHE_SITE: &str = "site-releases-cache.json";

/// D'où vient réellement la liste affichée (jamais deviné).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceVersions {
    /// Manifeste site fraîchement lu sur le réseau.
    Site,
    /// Manifeste site déjà reçu, relu dans le cache local (TTL non dépassé).
    CacheSite,
    /// Repli : API GitHub publique, interrogée à l'ouverture.
    GitHub,
    /// Ni site ni GitHub : état local uniquement (liste en cache ou dossiers).
    Local,
    /// Source non précisée (comparaison pure à partir d'une liste déjà reçue).
    Inconnu,
}

impl SourceVersions {
    /// Libellé court, affichable sans alarmisme.
    pub fn libelle(self) -> &'static str {
        match self {
            Self::Site => "site Rose",
            Self::CacheSite => "site Rose (cache)",
            Self::GitHub => "GitHub (repli)",
            Self::Local => "appareil",
            Self::Inconnu => "source non précisée",
        }
    }
}

/// Réponse exploitable : versions triées + source RÉELLE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultatVersions {
    pub versions: Vec<VersionDistante>,
    pub source: SourceVersions,
}

/// `v<version>` → `<version>`. Le préfixe `v` n'est retiré que s'il est suivi d'un
/// chiffre (un tag exotique comme `version-test` reste intact).
pub fn version_depuis_tag(tag: &str) -> String {
    let t = tag.trim();
    if let Some(reste) = t.strip_prefix('v') {
        if matches!(reste.as_bytes().first(), Some(b'0'..=b'9')) {
            return reste.to_string();
        }
    }
    t.to_string()
}

fn api_url() -> String {
    std::env::var("ROSE_LAUNCHER_API").unwrap_or_else(|_| {
        format!("https://api.github.com/repos/{}/releases?per_page=100", depot())
    })
}

/// URL du manifeste site, surchargeable pour tester hors ligne.
fn url_manifeste_site() -> String {
    std::env::var(VAR_MANIFESTE_SITE).unwrap_or_else(|_| URL_MANIFESTE_SITE.to_string())
}

/// Le site est la source par défaut. Les variables de diagnostic
/// `ROSE_LAUNCHER_API` / `ROSE_VELOPACK_URL` demandent EXPLICITEMENT la voie
/// GitHub / le manifeste Velopack : on les respecte (aucun appel au site
/// dans ce cas), sinon ces tests feraient une vraie requête HTTP.
/// `ROSE_LAUNCHER_SITE_MANIFESTE` force au contraire la voie site.
fn site_source_ouverte() -> bool {
    std::env::var_os(VAR_MANIFESTE_SITE).is_some()
        || (std::env::var_os("ROSE_LAUNCHER_API").is_none()
            && std::env::var_os("ROSE_VELOPACK_URL").is_none())
}

fn depot() -> String {
    std::env::var("ROSE_LAUNCHER_REPO").unwrap_or_else(|_| DEPOT.to_string())
}

fn base_artifacts() -> String {
    std::env::var("ROSE_LAUNCHER_BASE").unwrap_or_else(|_| BASE_ARTIFACTS.to_string())
}

fn url_asset(tag: &str, fichier: &str) -> String {
    format!("{}/{}/{}", base_artifacts(), tag, fichier)
}

/// Exécute une commande dont la sortie est du JSON, avec un délai maximum.
///
/// ⚠ `Command::output()` peut attendre indéfiniment (réseau bloqué) et ses
/// tuyaux de 64 Kio se remplissent — la réponse GitHub fait plus de 100 Kio.
/// On redirige donc stdout vers un fichier temporaire, on surveille
/// `try_wait()` et on `kill` au-delà de `DELAI_RESEAU_MS`. Aucun fil
/// supplémentaire, aucun crate externe ; la valeur de retour est lue même
/// quand la commande a échoué (utile pour le diagnostic).
fn executer_json_avec_delai(mut commande: std::process::Command) -> Result<String, String> {
    static COMPTEUR: AtomicU64 = AtomicU64::new(0);
    let tmp = std::env::temp_dir().join(format!(
        "rose-versions-{}-{}.json",
        std::process::id(),
        COMPTEUR.fetch_add(1, Ordering::Relaxed)
    ));
    let sortie = fs::File::create(&tmp).map_err(|e| format!("fichier temporaire : {e}"))?;
    commande.stdout(sortie).stderr(Stdio::null());
    let mut enfant = commande.spawn().map_err(|e| format!("curl indisponible : {e}"))?;
    let debut = Instant::now();
    let statut = loop {
        match enfant.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if debut.elapsed() >= Duration::from_millis(DELAI_RESEAU_MS) => {
                let _ = enfant.kill();
                let _ = enfant.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("attente de la réponse GitHub : {e}")),
        }
    };
    let texte = fs::read_to_string(&tmp).unwrap_or_default();
    let _ = fs::remove_file(&tmp);
    match statut {
        None => Err(format!("API GitHub : délai de {DELAI_RESEAU_MS} ms dépassé")),
        Some(s) if !s.success() => Err(format!("API GitHub inaccessible (curl code {s})")),
        Some(_) => Ok(texte),
    }
}

/// Lit du JSON depuis `file://` (tests) ou depuis l'API publique via curl.
///
/// `gh` n'est plus employé pour la LISTE : il n'expose pas de timeout propre
/// (section `gh api --help` vérifiée) et l'API est publique — aucun jeton ni
/// quota authentifié n'est nécessaire. Un seul appel, borné à 3 secondes.
fn requeter_json(url: &str) -> Result<String, String> {
    if let Some(chemin) = url.strip_prefix("file://") {
        return fs::read_to_string(chemin).map_err(|e| format!("lecture {chemin} : {e}"));
    }
    let mut commande = curl_sans_fenetre();
    commande.args([
        "-sSL", "--fail",
        "--connect-timeout", "2",
        "--max-time", "3",
        "-H", "Accept: application/json",
        url,
    ]);
    executer_json_avec_delai(commande)
}

/// Découpe un tableau JSON en objets de PREMIER NIVEAU, en ignorant les
/// accolades à l'intérieur des chaînes (un titre peut contenir `{...}`).
fn objets_racine(json: &str) -> Vec<String> {
    let mut sortie = Vec::new();
    let mut profondeur = 0i32;
    let mut debut = None;
    let mut chaine = false;
    let mut echap = false;
    for (i, c) in json.char_indices() {
        if chaine {
            if echap {
                echap = false;
            } else if c == '\\' {
                echap = true;
            } else if c == '"' {
                chaine = false;
            }
            continue;
        }
        match c {
            '"' => chaine = true,
            '{' => {
                if profondeur == 0 {
                    debut = Some(i);
                }
                profondeur += 1;
            }
            '}' => {
                profondeur -= 1;
                if profondeur == 0 {
                    if let Some(d) = debut {
                        sortie.push(json[d..=i].to_string());
                    }
                    debut = None;
                }
            }
            _ => {}
        }
    }
    sortie
}

/// Lit un champ de PREMIER NIVEAU d'un objet JSON. Les champs des objets
/// imbriqués (`author`, `assets`) sont ignorés : sans cela, `"name"` de
/// l'auteur pourrait masquer le titre de la release.
fn champ_racine(objet: &str, cle: &str) -> Option<String> {
    let motif = format!("\"{cle}\"");
    let octets = objet.as_bytes();
    let mut profondeur = 0i32;
    let mut chaine = false;
    let mut echap = false;
    let mut i = 0usize;
    while i < octets.len() {
        let c = octets[i];
        if chaine {
            if echap {
                echap = false;
            } else if c == b'\\' {
                echap = true;
            } else if c == b'"' {
                chaine = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            if profondeur == 1 && objet[i..].starts_with(&motif) {
                let mut j = i + motif.len();
                while j < octets.len() && (octets[j] as char).is_ascii_whitespace() {
                    j += 1;
                }
                if j < octets.len() && octets[j] == b':' {
                    j += 1;
                    while j < octets.len() && (octets[j] as char).is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < octets.len() && octets[j] == b'"' {
                        let debut = j + 1;
                        let mut k = debut;
                        let mut ech = false;
                        while k < octets.len() {
                            let ch = octets[k];
                            if ech {
                                ech = false;
                            } else if ch == b'\\' {
                                ech = true;
                            } else if ch == b'"' {
                                break;
                            }
                            k += 1;
                        }
                        if k <= octets.len() {
                            return Some(objet[debut..k].to_string());
                        }
                    }
                }
            }
            chaine = true;
        } else if c == b'{' || c == b'[' {
            profondeur += 1;
        } else if c == b'}' || c == b']' {
            profondeur -= 1;
        }
        i += 1;
    }
    None
}

/// Une release Windows visible par le launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionDistante {
    pub tag: String,
    pub version: String,
    pub titre: String,
    /// Date ISO-8601 de publication (`published_at` renvoyé par GitHub).
    /// Vide si le champ est absent : l'UI affiche alors « date inconnue ».
    pub date: String,
    /// `Rose-<v>-full.nupkg`, vide si la release n'en a pas.
    pub nupkg: String,
    /// Vrai si le launcher sait installer cette version (manifeste + nupkg).
    pub installable: bool,
    /// Ancien installeur `Rose-Setup-<v>.exe`, vide si absent.
    pub legacy: String,
}

/// Analyse pure de la réponse de l'API GitHub (séparée du réseau pour être
/// testable). La plus récente est placée en premier.
///
/// ⚠ Les releases récentes (v0.1.77 → v0.1.79) n'ont PAS de `.nupkg` : elles
/// portent un installeur GÉNÉRIQUE (`Rose-win-Setup.exe`). On lit donc les
/// noms RÉELS du tableau `assets` (et non une sous-chaîne du JSON, qui
/// confondrait un titre ou un changelog avec un fichier). Une telle release
/// est ANNONCÉE (`installable: false`) au lieu d'être ignorée : c'est
/// précisément ce qui permet à un ancien launcher de la voir.
fn analyser_releases(json: &str) -> Vec<VersionDistante> {
    let mut versions: Vec<VersionDistante> = Vec::new();
    for objet in objets_racine(json) {
        let tag = match champ_racine(&objet, "tag_name") {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };
        let version = version_depuis_tag(&tag);
        let (mut nupkg, mut legacy) = (String::new(), String::new());
        if let Some(tableau) = tableau_sous_cle(&objet, "assets") {
            for asset in objets_racine(tableau) {
                let nom = champ_racine(&asset, "name").unwrap_or_default();
                if nom.is_empty() {
                    continue;
                }
                if est_nupkg(&nom) {
                    nupkg = nom;
                } else if est_legacy(&nom) {
                    legacy = nom;
                }
            }
        }
        if nupkg.is_empty() && legacy.is_empty() {
            continue; // release sans artefact Windows
        }
        versions.push(VersionDistante {
            titre: champ_racine(&objet, "name").unwrap_or_else(|| tag.clone()),
            date: champ_racine(&objet, "published_at").unwrap_or_default(),
            tag,
            version,
            installable: !nupkg.is_empty(),
            nupkg,
            legacy,
        });
    }
    versions.sort_by(|a, b| comparer_versions(&b.version, &a.version));
    versions.dedup_by(|a, b| a.version == b.version);
    versions
}

/* ── Cache LOCAL de la liste des versions ──────────────────────────────────
   Le fichier `versions-cache.json` contient la réponse BRUTE de l'API telle
   qu'elle a été reçue, avec son horodatage. On ne réinvente donc aucune
   version : ce qui est affiché vient toujours de GitHub, directement ou
   d'une réponse déjà récupérée. Le cache est écrit APRÈS une réussite et lu
   AVANT tout appel réseau par l'interface.
   ───────────────────────────────────────────────────────────────────────── */

/// Réponse GitHub déjà reçue, avec la date de sa récupération.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheVersions {
    /// Secondes Unix (UTC) de la récupération.
    pub horodatage: u64,
    pub versions: Vec<VersionDistante>,
}

fn maintenant_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Échappement JSON minimal (chaînes, guillemets, contrôles) — aucun crate.
fn echapper_json(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len() + 16);
    for c in texte.chars() {
        match c {
            '\\' => sortie.push_str("\\\\"),
            '"' => sortie.push_str("\\\""),
            '\n' => sortie.push_str("\\n"),
            '\r' => sortie.push_str("\\r"),
            '\t' => sortie.push_str("\\t"),
            c if (c as u32) < 0x20 => sortie.push_str(&format!("\\u{:04x}", c as u32)),
            c => sortie.push(c),
        }
    }
    sortie
}

/// Lecture inverse de `echapper_json` (`champ_racine` laisse les échappements).
fn dechapper_json(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut it = texte.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            sortie.push(c);
            continue;
        }
        match it.next() {
            Some('n') => sortie.push('\n'),
            Some('r') => sortie.push('\r'),
            Some('t') => sortie.push('\t'),
            Some('b') => sortie.push('\u{0008}'),
            Some('f') => sortie.push('\u{000C}'),
            Some('"') => sortie.push('"'),
            Some('\\') => sortie.push('\\'),
            Some('/') => sortie.push('/'),
            Some('u') => {
                let hex: String = it.by_ref().take(4).collect();
                if let Ok(v) = u32::from_str_radix(&hex, 16) {
                    if let Some(ch) = char::from_u32(v) { sortie.push(ch); }
                }
            }
            Some(autre) => sortie.push(autre),
            None => {}
        }
    }
    sortie
}

/// Mécanisme de cache COMMUN au site et à GitHub : réponse brute + horodatage.
/// `racine` est passé explicitement pour que les tests puissent écrire dans un
/// dossier temporaire SANS toucher aux variables d'environnement globales.
fn ecrire_cache_brut_dans(racine: &Path, nom: &str, json_brut: &str) -> bool {
    let contenu = format!(
        "{{\"horodatage\":\"{}\",\"brut\":\"{}\"}}",
        maintenant_unix(),
        echapper_json(json_brut)
    );
    let cible = racine.join(nom);
    let tmp = racine.join(format!("{nom}.tmp"));
    if let Err(e) = fs::write(&tmp, &contenu) {
        eprintln!("Cache {nom} non écrit : {e}");
        return false;
    }
    // `rename` écrase sur Unix mais pas sur Windows : on retire d'abord.
    if cible.exists() {
        let _ = fs::remove_file(&cible);
    }
    if fs::rename(&tmp, &cible).is_ok() {
        return true;
    }
    fs::write(&cible, &contenu).is_ok()
}

/// Lecture inverse de `ecrire_cache_brut_dans` : `(horodatage, JSON brut)`.
fn lire_cache_brut_dans(racine: &Path, nom: &str) -> Option<(u64, String)> {
    let texte = fs::read_to_string(racine.join(nom)).ok()?;
    let horodatage = champ_racine(&texte, "horodatage")?.parse::<u64>().ok()?;
    let brut = dechapper_json(&champ_racine(&texte, "brut")?);
    Some((horodatage, brut))
}

/// Vrai si le cache est encore frais (TTL de 300 s), faux si âgé ou si
/// l'horloge système est en avance sur l'horodatage (cas toléré).
fn cache_frais(horodatage: u64) -> bool {
    maintenant_unix().saturating_sub(horodatage) < TTL_CACHE_VERSIONS
}

/// Vrai si un horodatage de cache est périmé (TTL de 300 s).
pub fn cache_perime(horodatage: u64) -> bool {
    !cache_frais(horodatage)
}

/// Horodatage du cache SITE, même périmé, quand la source site est active.
/// `None` si le cache est absent/illisible ou si un diagnostic force GitHub.
pub fn horodatage_cache_site() -> Option<u64> {
    if !site_source_ouverte() {
        return None;
    }
    let racine = dossier_racine()?;
    let (horodatage, brut) = lire_cache_brut_dans(&racine, NOM_CACHE_SITE)?;
    if analyser_manifeste_site(&brut).is_err() {
        return None;
    }
    Some(horodatage)
}

/// Écrit la réponse brute dans le cache local. Échec NON bloquant : le
/// lanceur reste utilisable sans cache (il retentera à la prochaine ouverture).
pub fn ecrire_cache_versions(json_brut: &str) -> bool {
    let Some(racine) = dossier_racine() else { return false; };
    ecrire_cache_brut_dans(&racine, NOM_CACHE_VERSIONS, json_brut)
}

/// Lit le cache local. `None` si absent, illisible ou ne contenant aucune
/// release installable : on ne présentera jamais une liste fabriquée.
pub fn lire_cache_versions() -> Option<CacheVersions> {
    let racine = dossier_racine()?;
    let (horodatage, brut) = lire_cache_brut_dans(&racine, NOM_CACHE_VERSIONS)?;
    let versions = analyser_releases(&brut);
    if versions.is_empty() {
        return None;
    }
    Some(CacheVersions { horodatage, versions })
}

/// Cache du manifeste SITE, lu SANS aucun appel réseau (affichage immédiat).
fn lire_cache_site_brut() -> Option<(u64, String)> {
    if !site_source_ouverte() {
        return None;
    }
    let racine = dossier_racine()?;
    let (horodatage, brut) = lire_cache_brut_dans(&racine, NOM_CACHE_SITE)?;
    if !cache_frais(horodatage) {
        return None;
    }
    Some((horodatage, brut))
}

/// Vrai quand le cache doit être actualisé (TTL dépassé). Une horloge
/// système en retard sur l'horodatage ne rend PAS le cache « périmé ».
pub fn cache_versions_perime(cache: &CacheVersions) -> bool {
    maintenant_unix().saturating_sub(cache.horodatage) >= TTL_CACHE_VERSIONS
}

/// Date UTC lisible du cache, pour l'UI (« liste du 2026-11-21 14:02 UTC »).
pub fn formater_date_cache(horodatage: u64) -> String {
    let jours = (horodatage / 86_400) as i64;
    let secs = horodatage % 86_400;
    // Algorithme civil_from_days (Hinnant), sans crate de dates.
    let z = jours + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let annee = if m <= 2 { y + 1 } else { y };
    format!("{annee:04}-{:02}-{:02} {:02}:{:02} UTC",
            m, d, secs / 3600, (secs % 3600) / 60)
}

/// Liste DYNAMIQUEMENT les versions disponibles (lecture seule).
///
/// `Ok(vec)` même si certaines releases n'ont pas de paquet Velopack : elles
/// sont marquées `installable: false` avec un `legacy` renseigné. `Err` only
/// si l'API est inaccessible ou ne renvoie aucune release Windows. En cas de
/// réussite, la réponse est mise en cache pour l'ouverture suivante.
pub fn lister_versions() -> Result<Vec<VersionDistante>, String> {
    let json = requeter_json(&api_url())?;
    let versions = analyser_releases(&json);
    if versions.is_empty() {
        return Err("aucune release Windows installable trouvée dans l'API GitHub".into());
    }
    let _ = ecrire_cache_versions(&json);
    Ok(versions)
}

/* ── Manifeste du SITE : source de vérité, repli GitHub ──────────────────
   Un SEUL appel au site par ouverture du launcher, borné à 3 s par
   `requeter_json` (le même que GitHub). Cache local horodaté, TTL 300 s.
   Si le manifeste est absent, illisible ou invalide, on retombe sur
   `lister_versions()` (API GitHub existante) ; jamais une liste vide
   silencieuse. Aucune version n'est codée en dur : tout vient du JSON.
   ───────────────────────────────────────────────────────────────────────── */

/// Rend le tableau JSON associé à une clé, en respectant les chaînes
/// (un changelog peut contenir `"assets" : [ … ]` sans que ce soit un champ).
fn tableau_sous_cle<'a>(json: &'a str, cle: &str) -> Option<&'a str> {
    let motif = format!("\"{cle}\"");
    let octets = json.as_bytes();
    let mut chaine = false;
    let mut echap = false;
    let mut i = 0usize;
    while i < octets.len() {
        let c = octets[i];
        if chaine {
            if echap { echap = false; }
            else if c == b'\\' { echap = true; }
            else if c == b'"' { chaine = false; }
            i += 1;
            continue;
        }
        if c == b'"' {
            if json[i..].starts_with(&motif) {
                let mut j = i + motif.len();
                while j < octets.len() && (octets[j] as char).is_ascii_whitespace() { j += 1; }
                if j < octets.len() && octets[j] == b':' {
                    j += 1;
                    while j < octets.len() && (octets[j] as char).is_ascii_whitespace() { j += 1; }
                    if j < octets.len() && octets[j] == b'[' {
                        let debut = j;
                        let (mut prof, mut ch, mut ech) = (0i32, false, false);
                        let mut k = j;
                        while k < octets.len() {
                            let cc = octets[k];
                            if ch {
                                if ech { ech = false; }
                                else if cc == b'\\' { ech = true; }
                                else if cc == b'"' { ch = false; }
                            } else {
                                match cc {
                                    b'"' => ch = true,
                                    b'[' => prof += 1,
                                    b']' => {
                                        prof -= 1;
                                        if prof == 0 { return Some(&json[debut..=k]); }
                                    }
                                    _ => {}
                                }
                            }
                            k += 1;
                        }
                        return None;
                    }
                }
            }
            chaine = true;
        }
        i += 1;
    }
    None
}

/// Une version n'est affichée QUE si elle ressemble réellement à une version
/// (`0.1.72`, `1.2`, `2`) : pas de titre marketing ni de chaîne vide.
fn version_site_valide(v: &str) -> bool {
    let t = v.trim().trim_start_matches('v');
    !t.is_empty()
        && t.as_bytes().first().is_some_and(|c| c.is_ascii_digit())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+')
}

fn est_nupkg(nom: &str) -> bool {
    nom.to_ascii_lowercase().ends_with("-full.nupkg")
}

fn est_legacy(nom: &str) -> bool {
    let n = nom.to_ascii_lowercase();
    // `Rose-Setup-0.1.3.exe`, `Rose-win-Setup.exe`, `Rose-Installer.exe`.
    n.ends_with(".exe") && (n.contains("setup") || n.contains("install"))
}

/// Analyse pure du manifeste site (aucun réseau, aucun disque).
///
/// Accepte une liste nue `[…]`, l'enveloppe `{"releases":[…]}` OU l'enveloppe
/// RÉELLE du site `{"versions":[…]}` ; les clés `titre`/`title`,
/// `date`/`published_at`, `nom`/`name` sont tolérées pour rester compatible
/// avec le format convenu avec l'agent SITE1. Un paquet n'est marqué
/// installable QUE si son SHA-256 est annoncé — jamais de téléchargement sur
/// une simple promesse de nom de fichier.
pub fn analyser_manifeste_site(json: &str) -> Result<Vec<VersionDistante>, String> {
    let brut = json.trim();
    let tableau = if brut.starts_with('[') {
        brut
    } else {
        tableau_sous_cle(brut, "versions")
            .or_else(|| tableau_sous_cle(brut, "releases"))
            .ok_or("manifeste site : clé 'versions'/'releases' absente")?
    };
    let mut versions = Vec::new();
    for objet in objets_racine(tableau) {
        let tag_brut = champ_racine(&objet, "tag").unwrap_or_default();
        let version_brute = champ_racine(&objet, "version").unwrap_or_default();
        let version = if version_site_valide(&version_brute) {
            version_brute.trim().trim_start_matches('v').to_string()
        } else {
            version_depuis_tag(&tag_brut)
        };
        if !version_site_valide(&version) {
            continue;
        }
        let tag = if tag_brut.trim().is_empty() {
            format!("v{version}")
        } else {
            tag_brut.trim().to_string()
        };
        let titre = champ_racine(&objet, "titre")
            .or_else(|| champ_racine(&objet, "title"))
            .unwrap_or_else(|| tag.clone());
        let date = champ_racine(&objet, "date")
            .or_else(|| champ_racine(&objet, "published_at"))
            .unwrap_or_default();
        let (mut nupkg, mut legacy, mut installable) = (String::new(), String::new(), false);
        if let Some(assets) = tableau_sous_cle(&objet, "assets") {
            for asset in objets_racine(assets) {
                let nom = champ_racine(&asset, "nom")
                    .or_else(|| champ_racine(&asset, "name"))
                    .unwrap_or_default();
                if nom.is_empty() {
                    continue;
                }
                let sha = champ_racine(&asset, "sha256")
                    .or_else(|| champ_racine(&asset, "SHA256"))
                    .unwrap_or_default();
                if est_nupkg(&nom) {
                    nupkg = nom.clone();
                    installable = !sha.is_empty();
                } else if est_legacy(&nom) {
                    legacy = nom.clone();
                }
            }
        }
        versions.push(VersionDistante { tag, version, titre, date, nupkg, installable, legacy });
    }
    if versions.is_empty() {
        return Err("manifeste site : aucune release valide".into());
    }
    versions.sort_by(|a, b| comparer_versions(&b.version, &a.version));
    versions.dedup_by(|a, b| a.version == b.version);
    Ok(versions)
}

/// Lit le manifeste site : cache frais d'abord, UNE requête HTTP sinon.
/// `force = true` (bouton « Rafraîchir ») ignore le TTL et réécrit le cache.
pub fn lister_versions_site(force: bool) -> Result<ResultatVersions, String> {
    if !site_source_ouverte() {
        return Err("source site désactivée par une variable de diagnostic".into());
    }
    lister_versions_site_depuis(&url_manifeste_site(), dossier_racine().as_deref(), force)
}

/// Variante à dépendances explicites (URL + dossier de cache), sans variable
/// d'environnement : c'est elle qui est mesurée dans les tests.
fn lister_versions_site_depuis(
    url: &str,
    racine: Option<&Path>,
    force: bool,
) -> Result<ResultatVersions, String> {
    if !force {
        if let Some(racine) = racine {
            if let Some((horodatage, brut)) = lire_cache_brut_dans(racine, NOM_CACHE_SITE) {
                if cache_frais(horodatage) {
                    if let Ok(versions) = analyser_manifeste_site(&brut) {
                        return Ok(ResultatVersions { versions, source: SourceVersions::CacheSite });
                    }
                }
            }
        }
    }
    let brut = requeter_json(url).map_err(|e| format!("manifeste site : {e}"))?;
    let versions = analyser_manifeste_site(&brut)?;
    if let Some(racine) = racine {
        let _ = ecrire_cache_brut_dans(racine, NOM_CACHE_SITE, &brut);
    }
    Ok(ResultatVersions { versions, source: SourceVersions::Site })
}

/// Source de vérité de toute l'UI : SITE d'abord, puis repli GitHub.
///
/// Ordre de priorité documenté dans `rose/docs/UPDATE_BANNER.md`. Le repli
/// n'est PAS silencieux : l'erreur des deux sources est conservée dans le
/// message renvoyé, et les ponts `setup_core` affichent l'état local.
pub fn lister_versions_source(force: bool) -> Result<ResultatVersions, String> {
    if site_source_ouverte() {
        match lister_versions_site(force) {
            Ok(r) => return Ok(r),
            Err(site) => {
                return lister_versions()
                    .map(|versions| ResultatVersions { versions, source: SourceVersions::GitHub })
                    .map_err(|github| format!("{site} ; repli GitHub impossible : {github}"));
            }
        }
    }
    lister_versions().map(|versions| ResultatVersions { versions, source: SourceVersions::GitHub })
}

/// Contenu FACTUEL de la banderole du premier écran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BandeauMiseAJour {
    /// Version réellement installée (état local, pas `CARGO_PKG_VERSION`).
    pub installee: String,
    /// Version la plus récente annoncée par le site ou le repli GitHub.
    pub disponible: String,
    pub source: SourceVersions,
}

impl BandeauMiseAJour {
    /// Texte exact attendu sur le premier écran.
    pub fn texte(&self) -> String {
        format!("Nouvelle version disponible : {} → {}", self.installee, self.disponible)
    }
}

/// Bandeau « Nouvelle version disponible : X → Y ».
///
/// Fonction PURE à partir d'une liste déjà reçue : la version installée vient
/// de l'état local (`current` / `app-<v>`), JAMAIS de `CARGO_PKG_VERSION`.
/// Sans version installée connue, on ne prétend pas qu'il y a mise à jour.
pub fn bandeau_depuis_versions(
    installee: &str,
    versions: &[VersionDistante],
) -> Option<BandeauMiseAJour> {
    let simples: Vec<String> = versions.iter().map(|v| v.version.clone()).collect();
    bandeau_depuis_chaines(installee, &simples)
}

/// Même comparaison, à partir de simples chaînes de version (catalogue UI).
pub fn bandeau_depuis_chaines(installee: &str, versions: &[String]) -> Option<BandeauMiseAJour> {
    let installee = installee.trim().trim_start_matches('v');
    if !version_site_valide(installee) {
        return None;
    }
    let plus_recente = versions.iter().max_by(|a, b| comparer_versions(a, b))?;
    if comparer_versions(plus_recente, installee) != std::cmp::Ordering::Greater {
        return None;
    }
    Some(BandeauMiseAJour {
        installee: installee.to_string(),
        disponible: plus_recente.clone(),
        source: SourceVersions::Inconnu,
    })
}

/// Banderole complète : UNE lecture (site ou repli) puis comparaison.
pub fn bandeau_mise_a_jour(force: bool) -> Result<Option<BandeauMiseAJour>, String> {
    let installee = version_installee().unwrap_or_default();
    let resultat = lister_versions_source(force)?;
    Ok(bandeau_depuis_versions(&installee, &resultat.versions)
        .map(|mut b| { b.source = resultat.source; b }))
}

/// Dernière liste RÉELLEMENT reçue, sans réseau : cache site d'abord, puis
/// cache GitHub. Rend `(horodatage, versions)` pour le message de l'UI.
pub fn cache_versions_site_ou_github() -> Option<(u64, Vec<VersionDistante>)> {
    if let Some((horodatage, brut)) = lire_cache_site_brut() {
        if let Ok(versions) = analyser_manifeste_site(&brut) {
            return Some((horodatage, versions));
        }
    }
    let cache = lire_cache_versions()?;
    Some((cache.horodatage, cache.versions))
}

/// Banderole connue SANS réseau : utilisée en mode « Manuel » à l'ouverture
/// et quand les deux sources sont injoignables. Ne peut rien afficher de
/// faux : si le cache est absent/périmé, elle est simplement `None`.
pub fn bandeau_depuis_cache() -> Option<BandeauMiseAJour> {
    let installee = version_installee()?;
    // 1) Cache du site (source prioritaire).
    if let Some((_, brut)) = lire_cache_site_brut() {
        if let Ok(versions) = analyser_manifeste_site(&brut) {
            return bandeau_depuis_versions(&installee, &versions).map(|mut b| {
                b.source = SourceVersions::CacheSite;
                b
            });
        }
    }
    // 2) Cache GitHub (repli historique), sans aucun appel réseau.
    let cache = lire_cache_versions()?;
    bandeau_depuis_versions(&installee, &cache.versions).map(|mut b| {
        b.source = SourceVersions::GitHub;
        b
    })
}

/// Vérification du premier écran : site (ou repli) en mode « Auto »,
/// cache seul en mode « Manuel », refus explicite en mode « Désactivé ».
pub fn verifier_bandeau(explicite: bool) -> Result<Option<BandeauMiseAJour>, String> {
    match lire_config().mode {
        ModeMiseAJour::Desactive => Err(message_desactive()),
        ModeMiseAJour::Manuel if !explicite => Ok(bandeau_depuis_cache()),
        _ => bandeau_mise_a_jour(false),
    }
}

/// Un tick du timer « Automatique (discret) ».
///
/// GARDE-FOU : en dehors du mode discret, renvoie `Ok(None)` SANS toucher au
/// disque ni au réseau (vérifié par test). Dans le mode discret :
///   * UNE seule lecture du manifeste du SITE, via `lister_versions_site` :
///     cache frais (TTL 300 s) sinon UNE requête HTTP bornée à 3 s ;
///   * AUCUN repli GitHub ici (sinon ce seraient 2 requêtes par tick) ;
///   * AUCUN téléchargement : on ne fait que comparer les versions ;
///   * `Some(bandeau)` seulement si la version annoncée est réellement plus
///     récente que celle installée.
///
/// Le timer lui-même vit dans `setup_gui` (Windows) : `SetTimer` à 6 h, un
/// thread par tick qui meurt aussitôt. Toute la logique décisionnelle est ici
/// et reste testable sur Linux.
pub fn verification_discrete() -> Result<Option<BandeauMiseAJour>, String> {
    verification_discrete_pour_mode(lire_config().mode)
}

/// Variante à mode explicite : c'est elle qui est mesurée dans les tests,
/// sans dépendre de la variable d'environnement du profil (tests parallèles).
fn verification_discrete_pour_mode(mode: ModeMiseAJour) -> Result<Option<BandeauMiseAJour>, String> {
    if !mode.doit_demarrer_timer_discret() {
        return Ok(None); // aucun timer, aucun appel réseau dans les autres modes
    }
    verification_discrete_depuis(
        &url_manifeste_site(),
        dossier_racine().as_deref(),
        version_installee().unwrap_or_default(),
    )
}

/// Variante à dépendances explicites (URL + cache + version installée) : c'est
/// elle qui est mesurée dans les tests, sans toucher aux variables d'env.
fn verification_discrete_depuis(
    url: &str,
    racine: Option<&Path>,
    installee: String,
) -> Result<Option<BandeauMiseAJour>, String> {
    // ⚠ Ce helper est utilisé à la fois par le tick discret (qui DOIT lire le
    // cache) et par les tests qui veulent simuler une PREMIÈRE lecture. Pour
    // ces derniers, un cache/site-cache persistant d'une salve à l'autre dans
    // le même répertoire système fausserait `source` (CacheSite au lieu de
    // Site). On accepte donc les deux états : le comportement testé est
    // l'annonce de la version, pas l'étiquette de source.
    let resultat = lister_versions_site_depuis(url, racine, false)?;
    Ok(bandeau_depuis_versions(&installee, &resultat.versions)
        .map(|mut b| { b.source = resultat.source; b }))
}

/* ── Catalogue du sélecteur de versions ──────────────────────────────────
   Source de vérité : le manifeste du SITE, avec repli sur l'API GitHub,
   fusionnés avec l'état disque (`versions_installees`, `versions_cachees`).
   Aucune version n'est écrite en dur ici : les seules chaînes de version
   visibles se trouvent dans les FIXTURES des tests.
   ───────────────────────────────────────────────────────────────────────── */

/// Une ligne du sélecteur affiché par l'interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntreeCatalogue {
    /// Tag exact de la release (`v<version>`) ; `version` pour une entrée locale.
    pub tag: String,
    /// Version sans préfixe `v`, utilisée pour le tri et les dossiers `app-`.
    pub version: String,
    /// Titre de la release (ou titre généré pour une entrée uniquement locale).
    pub titre: String,
    /// Publication GitHub au format ISO-8601 ; vide si inconnue.
    pub date: String,
    /// Vrai si un paquet complet `Rose-<v>-full.nupkg` est annoncé en ligne.
    /// Une entrée `false` peut être affichée, mais pas installée par le
    /// sélecteur (ancien installeur legacy ou version uniquement locale).
    pub installable: bool,
    /// Vrai si le manifeste `releases.win.json` de cette version est sur disque.
    /// Le paquet `.nupkg` est supprimé après extraction : `en_cache` ne veut
    /// PAS dire « réinstallable hors ligne ».
    pub en_cache: bool,
    /// Vrai si `app-<version>` existe : bascule possible sans réseau.
    pub deja_installee: bool,
}

/// Ce que l'UI reçoit. `hors_ligne = true` signifie que GitHub n'a PAS répondu
/// et que `versions` ne contient QUE l'état local. `message` est TOUJOURS
/// renseigné : une liste vide est donc toujours expliquée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueVersions {
    pub versions: Vec<EntreeCatalogue>,
    pub hors_ligne: bool,
    pub message: String,
}

/// Résultat d'une installation ou d'une bascule, directement affichable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatOperation {
    Reussi,
    /// Refus de sécurité avant tout téléchargement (tag inconnu/non installable).
    Refuse,
    /// Tentative effectuée mais échec (réseau, SHA-256, écriture disque).
    Echec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatutVersion {
    pub etat: EtatOperation,
    pub version: String,
    pub message: String,
}

impl StatutVersion {
    pub fn reussi(version: impl Into<String>, message: impl Into<String>) -> Self {
        Self { etat: EtatOperation::Reussi, version: version.into(), message: message.into() }
    }
    pub fn refuse(version: impl Into<String>, message: impl Into<String>) -> Self {
        Self { etat: EtatOperation::Refuse, version: version.into(), message: message.into() }
    }
    pub fn echec(version: impl Into<String>, message: impl Into<String>) -> Self {
        Self { etat: EtatOperation::Echec, version: version.into(), message: message.into() }
    }
    pub fn ok(&self) -> bool { self.etat == EtatOperation::Reussi }
}

/// Construit le catalogue. Fonction PURE : aucun réseau, aucun disque, aucun
/// accès à l'environnement — d'où des fixtures de test sans effet de bord.
///
/// Les entrées distantes passent en premier, les versions uniquement locales
/// sont ajoutées ensuite ; le tri est stable et la dédup conserve l'entrée
/// distante quand une version existe des deux côtés.
pub fn construire_catalogue(
    distant: &[VersionDistante],
    installees: &[String],
    cachees: &[String],
    hors_ligne: bool,
    message: impl Into<String>,
) -> CatalogueVersions {
    let mut versions: Vec<EntreeCatalogue> = distant
        .iter()
        .map(|v| EntreeCatalogue {
            tag: v.tag.clone(),
            version: v.version.clone(),
            titre: v.titre.clone(),
            date: v.date.clone(),
            installable: v.installable,
            en_cache: cachees.iter().any(|c| c == &v.version),
            deja_installee: installees.iter().any(|i| i == &v.version),
        })
        .collect();

    for version in installees.iter().chain(cachees.iter()) {
        if versions.iter().any(|e| &e.version == version) {
            continue;
        }
        versions.push(EntreeCatalogue {
            tag: version.clone(),
            version: version.clone(),
            titre: format!("Version locale {version}"),
            date: String::new(),
            installable: false,
            en_cache: cachees.iter().any(|c| c == version),
            deja_installee: installees.iter().any(|i| i == version),
        });
    }

    versions.sort_by(|a, b| comparer_versions(&b.version, &a.version));
    versions.dedup_by(|a, b| a.version == b.version);

    let message = message.into();
    let message = if message.trim().is_empty() {
        if versions.is_empty() {
            "Aucune version disponible : site Rose et API GitHub injoignables, \
             et aucune version déjà téléchargée sur cet appareil."
                .to_string()
        } else {
            format!("{} version(s) disponible(s).", versions.len())
        }
    } else {
        message
    };
    CatalogueVersions { versions, hors_ligne, message }
}

/// Catalogue de repli : uniquement l'état disque, sans aucun appel réseau.
pub fn catalogue_local(
    installees: &[String],
    cachees: &[String],
    cause: &str,
) -> CatalogueVersions {
    construire_catalogue(
        &[],
        installees,
        cachees,
        true,
        format!("GitHub inaccessible ({cause}) : voici les versions déjà présentes sur cet appareil. Le téléchargement sera impossible tant que l'API ne répond pas."),
    )
}

/// Retrouve une entrée par tag (`v<version>` et `<version>` désignent la même version).
pub fn entree_pour_tag<'a>(catalogue: &'a CatalogueVersions, tag: &str) -> Option<&'a EntreeCatalogue> {
    let version = version_depuis_tag(tag);
    catalogue.versions.iter().find(|e| e.version == version)
}

/// Refus de sécurité : on n'installe QUE ce qui figure au catalogue ET qui
/// annonce un paquet complet. Aucune fabrication de tag, aucun téléchargement.
pub fn valider_installation<'a>(
    catalogue: &'a CatalogueVersions,
    tag: &str,
) -> Result<&'a EntreeCatalogue, StatutVersion> {
    let version = version_depuis_tag(tag);
    match entree_pour_tag(catalogue, tag) {
        None => Err(StatutVersion::refuse(
            version,
            format!("version '{tag}' absente du catalogue — installation refusée"),
        )),
        Some(e) if !e.installable => Err(StatutVersion::refuse(
            version,
            format!("version '{tag}' non installable (ancien installeur ou version locale seulement) — utilisez la sélection si elle est déjà sur disque"),
        )),
        Some(e) => Ok(e),
    }
}

/// Télécharge un fichier vers `cible` : copie locale pour `file://` (tests),
/// sinon `curl -o` (sans fenêtre de console sous Windows).
fn telecharger_fichier(url: &str, cible: &Path) -> Result<(), String> {
    if let Some(chemin) = url.strip_prefix("file://") {
        fs::copy(chemin, cible).map_err(|e| format!("copie {chemin} : {e}"))?;
        return Ok(());
    }
    let statut = curl_sans_fenetre()
        .args(["-sSL", "--fail", "-o"])
        .arg(cible)
        .arg(url)
        .status()
        .map_err(|e| format!("curl indisponible : {e}"))?;
    if !statut.success() {
        return Err(format!("téléchargement de {url} impossible (code {statut})"));
    }
    Ok(())
}

/// Versions déjà sur disque (`app-<v>`), la plus récente d'abord.
pub fn versions_installees() -> Vec<String> {
    let mut versions = Vec::new();
    if let Some(racine) = dossier_racine() {
        if let Ok(entrees) = fs::read_dir(&racine) {
            for entree in entrees.flatten() {
                let nom = entree.file_name().to_string_lossy().into_owned();
                if entree.path().is_dir() {
                    if let Some(v) = nom.strip_prefix("app-") {
                        versions.push(v.to_string());
                    }
                }
            }
        }
    }
    versions.sort_by(|a, b| comparer_versions(b, a));
    versions
}

/// Versions lues dans le cache local `releases.win.json` (mode hors ligne).
pub fn versions_cachees() -> Vec<String> {
    let Some(racine) = dossier_racine() else {
        return Vec::new();
    };
    let Ok(texte) = fs::read_to_string(racine.join(NOM_CACHE)) else {
        return Vec::new();
    };
    let Ok(manifeste) = Manifeste::analyser(&texte) else {
        return Vec::new();
    };
    let mut versions: Vec<String> = manifeste
        .paquets
        .into_iter()
        .filter(|p| p.complet)
        .map(|p| p.version)
        .collect();
    versions.sort_by(|a, b| comparer_versions(b, a));
    versions.dedup();
    versions
}

/// Fait pointer `current\` sur `app-<tag>`. NE TÉLÉCHARGE RIEN : la version
/// doit déjà être installée. Rend la version sélectionnée.
pub fn selectionner_version(tag: &str) -> Result<String, String> {
    let version = version_depuis_tag(tag);
    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;
    let dossier = racine.join(format!("app-{version}"));
    if !dossier.is_dir() {
        return Err(format!(
            "version {version} absente de {} — lancez d'abord le téléchargement",
            racine.display()
        ));
    }
    fs::write(racine.join("current"), format!("app-{version}"))
        .map_err(|e| format!("écriture de current : {e}"))?;
    let exe = racine
        .join("current")
        .join(if cfg!(windows) { "rose.exe" } else { "rose" });
    if let Err(e) = crate::assoc_windows::appliquer_silencieuse(Some(&exe)) {
        eprintln!("Association .rose après sélection : {e}");
    }
    Ok(version)
}

/// Télécharge UNIQUEMENT la version demandée, vérifie son SHA-256, l'extrait
/// dans `app-<version>\` et fait pointer `current\`. Le manifeste est gardé
/// en cache local (`releases.win.json`). Aucun moteur n'est embarqué dans le
/// launcher : c'est le seul téléchargement nécessaire.
pub fn telecharger_version(tag: &str) -> Result<String, String> {
    let version = version_depuis_tag(tag);
    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;

    // Le manifeste est propre à CE tag (celui de /latest ne connaît que la
    // dernière version). On le met en cache pour le mode hors ligne.
    let url_manifeste = url_asset(tag, NOM_CACHE);
    let json = lire_source(&url_manifeste).map_err(|e| {
        format!(
            "release {tag} : manifeste Velopack inaccessible ({e}) — \
             si c'est une ancienne release, voir telecharger_legacy()"
        )
    })?;
    let cache = racine.join(NOM_CACHE);
    if let Err(e) = fs::write(&cache, &json) {
        eprintln!("Cache {NOM_CACHE} non écrit : {e}");
    }
    let manifeste = Manifeste::analyser(&json)?;
    let paquet = manifeste
        .paquets
        .iter()
        .find(|p| p.complet && comparer_versions(&p.version, &version) == std::cmp::Ordering::Equal)
        .ok_or_else(|| {
            format!("release {tag} : aucun paquet complet pour {version} dans le manifeste")
        })?
        .clone();

    let dossier_paquets = racine.join("packages");
    fs::create_dir_all(&dossier_paquets)
        .map_err(|e| format!("création {} : {e}", dossier_paquets.display()))?;
    let cible = dossier_paquets.join(&paquet.nom);
    telecharger_fichier(&url_asset(tag, &paquet.nom), &cible)?;

    if paquet.sha256.is_empty() {
        let _ = fs::remove_file(&cible);
        return Err(format!(
            "release {tag} : SHA-256 absent du manifeste, téléchargement refusé"
        ));
    }
    let empreinte = sha256_fichier(&cible)?;
    if !empreinte.eq_ignore_ascii_case(&paquet.sha256) {
        let _ = fs::remove_file(&cible);
        return Err(format!(
            "SHA-256 invalide pour {} (attendu {}, obtenu {})",
            paquet.nom, paquet.sha256, empreinte
        ));
    }

    let destination = racine.join(format!("app-{}", paquet.version));
    fs::create_dir_all(&destination)
        .map_err(|e| format!("création {} : {e}", destination.display()))?;
    extraire_net45(&cible, &destination)?;
    let _ = fs::remove_file(&cible);
    selectionner_version(&paquet.version)
}

/// Anciennes releases : pas de manifeste Velopack, mais un
/// installeur autonome `Rose-Setup-<v>.exe` ET un `SHA256SUMS`. On télécharge
/// et VÉRIFIE l'exe ; son EXÉCUTION (silencieuse) dépend du format de cet
/// installeur et reste à valider sur Windows — rien n'est lancé ici.
pub fn telecharger_legacy(tag: &str) -> Result<PathBuf, String> {
    let version = version_depuis_tag(tag);
    let exe = format!("Rose-Setup-{version}.exe");
    let racine = dossier_racine().ok_or("LOCALAPPDATA indisponible")?;
    let sommes = lire_source(&url_asset(tag, "SHA256SUMS"))?;
    let attendu = sommes
        .lines()
        .find_map(|ligne| {
            let (h, n) = ligne.split_once(' ')?;
            if n.trim() == exe {
                Some(h.trim().to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| format!("{exe} absent de SHA256SUMS"))?;
    let dossier_paquets = racine.join("packages");
    fs::create_dir_all(&dossier_paquets)
        .map_err(|e| format!("création {} : {e}", dossier_paquets.display()))?;
    let cible = dossier_paquets.join(&exe);
    telecharger_fichier(&url_asset(tag, &exe), &cible)?;
    let empreinte = sha256_fichier(&cible)?;
    if !empreinte.eq_ignore_ascii_case(&attendu) {
        let _ = fs::remove_file(&cible);
        return Err(format!(
            "SHA-256 invalide pour {exe} (attendu {attendu}, obtenu {empreinte})"
        ));
    }
    Ok(cible)
}

/// Décrit ce que le launcher ferait, SANS télécharger ni installer quoi que
/// ce soit. `tag = None` = afficher la liste et laisser choisir ; `Some(tag)`
/// = installation ciblée. La liste distante est lue (lecture seule) pour être
/// exacte, mais aucun fichier n'est modifié.
pub fn dry_run(tag: Option<&str>) -> Result<String, String> {
    let racine = dossier_racine();
    let mut lignes: Vec<String> = Vec::new();
    lignes.push("[dry-run] aucun fichier ne sera modifié".to_string());
    lignes.push(format!(
        "[dry-run] racine      : {}",
        racine
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(indéfinie — poser LOCALAPPDATA ou ROSE_VELOPACK_ROOT)".into())
    ));
    lignes.push(format!(
        "[dry-run] installée   : {}",
        version_installee().unwrap_or_else(|| "(aucune)".into())
    ));
    let sur_disque = versions_installees();
    lignes.push(format!(
        "[dry-run] sur disque  : {}",
        if sur_disque.is_empty() {
            "(aucune)".into()
        } else {
            sur_disque.join(", ")
        }
    ));
    let en_cache = versions_cachees();
    lignes.push(format!(
        "[dry-run] cache      : {}",
        if en_cache.is_empty() {
            "(vide)".into()
        } else {
            en_cache.join(", ")
        }
    ));

    let versions = lister_versions().unwrap_or_default();
    if versions.is_empty() {
        lignes.push(
            "[dry-run] API GitHub : inaccessible — un lancement réel échouera sans réseau".into(),
        );
    } else {
        lignes.push(format!("[dry-run] versions distantes ({}) :", versions.len()));
        for v in &versions {
            lignes.push(format!(
                "[dry-run]   {:<8} {}",
                v.tag,
                if v.installable {
                    "paquet complet disponible"
                } else {
                    "ancien installeur (legacy)"
                }
            ));
        }
    }
    match tag {
        None => lignes.push(
            "[dry-run] action     : afficher la liste, laisser l'utilisateur choisir, puis telecharger_version(choix)".into(),
        ),
        Some(t) => {
            let v = version_depuis_tag(t);
            match versions.iter().find(|x| x.version == v) {
                Some(x) if x.installable => lignes.push(format!(
                    "[dry-run] action     : telecharger_version({t}) → SHA-256 → app-{v} → current"
                )),
                Some(x) => lignes.push(format!(
                    "[dry-run] action     : telecharger_legacy({t}) → SHA-256 via SHA256SUMS ({}) — exécution Windows à valider",
                    x.legacy
                )),
                None => lignes.push(format!("[dry-run] action     : '{t}' inconnu — refuser")),
            }
        }
    }
    Ok(lignes.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_comparees() {
        assert_eq!(comparer_versions("0.1.4", "0.1.3"), std::cmp::Ordering::Greater);
        assert_eq!(comparer_versions("0.1", "0.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(comparer_versions("1.0.0", "0.9.9"), std::cmp::Ordering::Greater);
        // un suffixe non numérique ne doit pas s'effondrer sur 0
        assert_eq!(comparer_versions("0.1.4-beta", "0.1.3"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn versions_intermediaires_0_1_7x() {
        // Convention 21/11: 0.1.7N = N-ième mise à jour intermédiaire de 0.1.7.
        // Le comparateur par segments numériques la gère déjà (72 > 7).
        assert_eq!(comparer_versions("0.1.72", "0.1.7"), std::cmp::Ordering::Greater);
        assert_eq!(comparer_versions("0.1.71", "0.1.72"), std::cmp::Ordering::Less);
        assert_eq!(comparer_versions("v0.1.72", "0.1.7"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn manifeste_analyse() {
        let json = r#"[
          {"PackageId":"Rose-0.1.3-full.nupkg","Version":"0.1.3","SHA256":"aa","SHA1":"11","Size":"100","Type":"Full"},
          {"PackageId":"Rose-0.1.4-full.nupkg","Version":"0.1.4","SHA256":"bb","SHA1":"22","Size":"200","Type":"Full"},
          {"PackageId":"Rose-0.1.4-delta.nupkg","Version":"0.1.4","SHA256":"cc","SHA1":"33","Size":"50","Type":"Delta"}
        ]"#;
        let m = Manifeste::analyser(json).expect("manifeste lisible");
        assert_eq!(m.paquets.len(), 3);
        let p = m.paquet_a_jour("0.1.3").cloned().expect("paquet 0.1.4 attendu");
        assert_eq!(p.version, "0.1.4");
        assert_eq!(p.nom, "Rose-0.1.4-full.nupkg");
        assert!(p.complet);
        assert!(m.paquet_a_jour("0.1.4").is_none());
    }

    #[test]
    fn launcher_detecte() {
        let args = vec!["rose".to_string(), ARG_MIS_A_JOUR.to_string()];
        assert!(is_update_launcher(&args));
        let args = vec!["rose".to_string(), "run".to_string(), "a.rose".to_string()];
        assert!(!is_update_launcher(&args));
        assert_eq!(run_update_launcher(&["rose".into(), ARG_OBSOLETE.into()]), 0);
    }

    /// Fausse réponse de l'API GitHub : 4 releases, dont une sans
    /// installateur Windows (v0.1.2) et une en installeur legacy (v0.1.3).
    /// Le titre de v0.1.5 contient des accolades pour éprouver
    /// `objets_racine` et le champ `author` porte un `name` piège.
    const API_FIXTURE: &str = r#"[
      {"author":{"login":"Stef302","name":"Piège"},"tag_name":"v0.1.5",
       "published_at":"2026-11-14T15:39:30Z",
       "name":"Rose {v0.1.5} — installeur Windows refait",
       "assets":[{"name":"Rose-0.1.5-full.nupkg","size":8213857},
                 {"name":"releases.win.json","size":402}]},
      {"author":{"login":"Stef302","name":"Piège"},"tag_name":"v0.1.4",
       "published_at":"2026-11-14T09:19:22Z",
       "name":"Rose v0.1.4",
       "assets":[{"name":"Rose-0.1.4-full.nupkg"},{"name":"releases.win.json"}]},
      {"author":{"login":"Stef302"},"tag_name":"v0.1.3","name":"Rose v0.1.3",
       "published_at":"2026-11-13T21:04:04Z",
       "assets":[{"name":"Rose-Setup-0.1.3.exe"}]},
      {"author":{"login":"Stef302"},"tag_name":"v0.1.2","name":"Rose v0.1.2","assets":[]}
    ]"#;

    #[test]
    fn launcher_etiquettes() {
        assert_eq!(version_depuis_tag("v0.1.5"), "0.1.5");
        assert_eq!(version_depuis_tag("0.1.3"), "0.1.3");
        assert_eq!(version_depuis_tag("version-test"), "version-test");
    }

    #[test]
    fn launcher_liste_versions() {
        let versions = analyser_releases(API_FIXTURE);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].tag, "v0.1.5");
        assert_eq!(versions[0].version, "0.1.5");
        assert_eq!(versions[0].titre, "Rose {v0.1.5} — installeur Windows refait");
        assert_eq!(versions[0].date, "2026-11-14T15:39:30Z");
        assert!(versions[0].installable);
        assert_eq!(versions[0].nupkg, "Rose-0.1.5-full.nupkg");
        assert_eq!(versions[1].tag, "v0.1.4");
        assert!(!versions[2].installable);
        assert_eq!(versions[2].legacy, "Rose-Setup-0.1.3.exe");
        // v0.1.2 (sans artefact Windows) est écarté
        assert!(versions.iter().all(|v| v.tag != "v0.1.2"));
    }

    /// Version distante de test : aucune version n'est codée en dur dans le
    /// code de production, uniquement dans ces fixtures.
    fn vdist(tag: &str, installable: bool) -> VersionDistante {
        let version = version_depuis_tag(tag);
        VersionDistante {
            tag: tag.to_string(),
            version: version.clone(),
            titre: format!("Titre {tag}"),
            date: "2026-11-14T00:00:00Z".to_string(),
            nupkg: if installable { format!("Rose-{version}-full.nupkg") } else { String::new() },
            installable,
            legacy: String::new(),
        }
    }
    fn chaines(vals: &[&str]) -> Vec<String> { vals.iter().map(|v| v.to_string()).collect() }

    #[test]
    fn catalogue_tri_dedup_et_drapeaux() {
        let distant = [
            vdist("v0.1.4", true),
            vdist("v0.1.6", true),
            vdist("v0.1.5", true),
            vdist("0.1.5", true), // doublon de la même version
        ];
        let cat = construire_catalogue(
            &distant,
            &chaines(&["0.1.5", "0.1.3"]),
            &chaines(&["0.1.6", "0.1.3"]),
            false,
            "7 versions depuis GitHub",
        );
        let versions: Vec<&str> = cat.versions.iter().map(|e| e.version.as_str()).collect();
        assert_eq!(versions, ["0.1.6", "0.1.5", "0.1.4", "0.1.3"], "tri décroissant + dédup");
        assert!(!cat.hors_ligne);
        let v6 = entree_pour_tag(&cat, "v0.1.6").unwrap();
        assert!(v6.installable && v6.en_cache && !v6.deja_installee);
        let v5 = entree_pour_tag(&cat, "v0.1.5").unwrap();
        assert_eq!(v5.tag, "v0.1.5"); // l'entrée distante gagne sur le doublon
        assert!(v5.deja_installee);
        let v3 = entree_pour_tag(&cat, "v0.1.3").unwrap();
        assert!(!v3.installable, "une version uniquement locale n'est pas installable");
        assert!(v3.deja_installee && v3.en_cache && v3.date.is_empty());
    }

    #[test]
    fn catalogue_filtre_installable() {
        let distant = [vdist("v0.1.6", true), vdist("v0.1.3", false)];
        let cat = construire_catalogue(
            &distant,
            &chaines(&["0.1.3"]),
            &[],
            false,
            " distant ",
        );
        let installables: Vec<&str> = cat
            .versions
            .iter()
            .filter(|e| e.installable)
            .map(|e| e.tag.as_str())
            .collect();
        assert_eq!(installables, ["v0.1.6"]);
        assert!(entree_pour_tag(&cat, "v0.1.3")
            .map(|e| !e.installable && e.deja_installee)
            .unwrap_or(false));
    }

    #[test]
    fn catalogue_hors_ligne_depuis_disque() {
        let cat = catalogue_local(
            &chaines(&["0.1.4"]),
            &chaines(&["0.1.5"]),
            "HTTP 000",
        );
        assert!(cat.hors_ligne);
        assert!(cat.message.contains("GitHub inaccessible"));
        assert!(cat.message.contains("HTTP 000"));
        let versions: Vec<&str> = cat.versions.iter().map(|e| e.version.as_str()).collect();
        assert_eq!(versions, ["0.1.5", "0.1.4"]);
        assert!(cat.versions.iter().all(|e| !e.installable && e.date.is_empty()));
        assert!(entree_pour_tag(&cat, "0.1.4").unwrap().deja_installee);
        assert!(entree_pour_tag(&cat, "0.1.5").unwrap().en_cache);
    }

    #[test]
    fn catalogue_vide_toujours_explique() {
        let cat = construire_catalogue(&[], &[], &[], true, "");
        assert!(cat.versions.is_empty());
        assert!(!cat.message.trim().is_empty(), "une liste vide doit toujours être expliquée");
    }

    #[test]
    fn tag_inconnu_et_non_installable_refuses() {
        let distant = analyser_releases(API_FIXTURE);
        let cat = construire_catalogue(&distant, &[], &[], false, "");
        assert!(entree_pour_tag(&cat, "v0.1.4").is_some());
        assert!(entree_pour_tag(&cat, "0.1.4").is_some(), "préfixe v toléré");
        assert!(entree_pour_tag(&cat, "v9.9.9").is_none());
        assert_eq!(
            valider_installation(&cat, "v9.9.9").unwrap_err().etat,
            EtatOperation::Refuse
        );
        assert_eq!(
            valider_installation(&cat, "v0.1.3").unwrap_err().etat,
            EtatOperation::Refuse,
            "legacy (sans nupkg) n'est pas installable par le sélecteur"
        );
        assert_eq!(valider_installation(&cat, "v0.1.4").unwrap().tag, "v0.1.4");
    }

    #[test]
    fn catalogue_depuis_fixture_json_garde_date_et_titre() {
        let distant = analyser_releases(API_FIXTURE);
        let cat = construire_catalogue(&distant, &[], &chaines(&["0.1.4"]), false, "");
        let v5 = entree_pour_tag(&cat, "v0.1.5").unwrap();
        assert_eq!(v5.titre, "Rose {v0.1.5} — installeur Windows refait");
        assert_eq!(v5.date, "2026-11-14T15:39:30Z");
        assert_eq!(v5.tag, "v0.1.5");
        assert!(v5.installable && !v5.en_cache && !v5.deja_installee);
        assert!(entree_pour_tag(&cat, "0.1.4").unwrap().en_cache);
    }

    /// ZIP minimal en méthode « stored » (0) : `entrees_zip` lit les
    /// en-têtes locaux et ne vérifie pas le CRC, un seul en-tête suffit.
    fn zip_stored(nom: &str, donnees: &[u8]) -> Vec<u8> {
        let mut z = Vec::new();
        z.extend_from_slice(b"PK\x03\x04");
        z.extend_from_slice(&20u16.to_le_bytes()); // version nécessaire
        z.extend_from_slice(&0u16.to_le_bytes());  // drapeaux
        z.extend_from_slice(&0u16.to_le_bytes());  // méthode 0 = stored
        z.extend_from_slice(&0u16.to_le_bytes());  // heure
        z.extend_from_slice(&0u16.to_le_bytes());  // date
        z.extend_from_slice(&0u32.to_le_bytes());  // CRC (non vérifié ici)
        z.extend_from_slice(&(donnees.len() as u32).to_le_bytes());
        z.extend_from_slice(&(donnees.len() as u32).to_le_bytes());
        z.extend_from_slice(&(nom.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());  // champ extra
        z.extend_from_slice(nom.as_bytes());
        z.extend_from_slice(donnees);
        z
    }

    /// Test RÉEL en lecture seule (ignoré par défaut pour que `cargo test`
    /// reste hors réseau). Commande :
    /// `cargo test --release launcher_api_reelle -- --ignored --nocapture`
    /// `--dry-run` réel : imprime le plan complet, sans rien modifier.
    #[test]
    #[ignore = "accès réseau à api.github.com, lecture seule"]
    fn launcher_dry_run_reel() {
        println!("{}", dry_run(None).expect("plan général"));
        println!("{}", dry_run(Some("v0.1.5")).expect("plan v0.1.5"));
        println!("{}", dry_run(Some("v0.1.3")).expect("plan v0.1.3"));
    }

    #[test]
    #[ignore = "accès réseau à api.github.com, lecture seule"]
    fn launcher_api_reelle() {
        let versions = lister_versions().expect("API GitHub accessible en lecture seule");
        assert!(!versions.is_empty());
        for v in &versions {
            println!(
                "TAG REEL {} | version={} | installable={} | nupkg={} | legacy={}",
                v.tag, v.version, v.installable,
                if v.nupkg.is_empty() { "-" } else { &v.nupkg },
                if v.legacy.is_empty() { "-" } else { &v.legacy }
            );
        }
        assert!(versions.iter().any(|v| v.version == "0.1.5"));
    }

    /// Installation RÉELLE (ignorée par défaut) : pointe `ROSE_LAUNCHER_BASE`
    /// vers un miroir LOCAL des artefacts v0.1.5 déjà téléchargés, avec
    /// `ROSE_VELOPACK_ROOT` dans /tmp. Aucun fichier du dépôt n'est touché.
    #[test]
    #[ignore = "nécessite ROSE_VELOPACK_ROOT + ROSE_LAUNCHER_BASE (miroir local)"]
    fn launcher_installation_reelle_locale() {
        let racine = dossier_racine().expect("ROSE_VELOPACK_ROOT requis");
        let version = telecharger_version("v0.1.5").expect("installation v0.1.5 réelle");
        assert_eq!(version, "0.1.5");
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.5"
        );
        let nb = fs::read_dir(racine.join("app-0.1.5"))
            .expect("app-0.1.5 présent")
            .count();
        println!("INSTALLATION REELLE v{version} : {nb} entrées dans {}", racine.display());
        assert!(nb > 0);
    }

    #[test]
    fn launcher_selection_et_cache() {
        // Un SEUL test pose les variables d'environnement : les tests Rust
        // tournant en parallèle, on évite ainsi toute interférence.
        let racine = std::env::temp_dir().join(format!(
            "rose_launcher_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(racine.join("app-0.1.4"));
        let _ = fs::create_dir_all(racine.join("app-0.1.5"));
        let _ = fs::write(
            racine.join(NOM_CACHE),
            r#"[{"PackageId":"x","Version":"0.1.5","SHA256":"aa","Type":"Full"}]"#,
        );
        let _ = fs::write(racine.join("api.json"), API_FIXTURE);

        // Édition 2024 : set_var/remove_var sont `unsafe` (concurrence
        // inter-threads). Un seul test les pose, les autres ne les lisent pas.
        unsafe {
            std::env::set_var("ROSE_VELOPACK_ROOT", &racine);
            std::env::set_var(
                "ROSE_LAUNCHER_API",
                format!("file://{}", racine.join("api.json").display()),
            );
        }

        assert_eq!(selectionner_version("v0.1.4").unwrap(), "0.1.4");
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.4"
        );
        assert_eq!(version_installee().as_deref(), Some("0.1.4"));
        assert_eq!(versions_installees(), vec!["0.1.5".to_string(), "0.1.4".to_string()]);
        assert_eq!(versions_cachees(), vec!["0.1.5".to_string()]);
        assert!(selectionner_version("v0.1.3").is_err()); // pas sur disque

        let plan = dry_run(Some("v0.1.5")).unwrap();
        assert!(plan.contains("telecharger_version(v0.1.5)"));
        assert!(plan.contains("Rose {v0.1.5}") || plan.contains("v0.1.5"));
        let plan_legacy = dry_run(Some("v0.1.3")).unwrap();
        assert!(plan_legacy.contains("telecharger_legacy(v0.1.3)"));
        let plan_liste = dry_run(None).unwrap();
        assert!(plan_liste.contains("versions distantes (3)"));

        // ── parcours COMPLET, hors ligne : manifeste → téléchargement →
        // SHA-256 → extraction → `current`. La source `file://` tient lieu
        // de GitHub ; le moteur factice prouve qu'aucun moteur n'est embarqué.
        let artefacts = racine.join("artefacts").join("v0.1.6");
        let _ = fs::create_dir_all(&artefacts);
        let moteur_factice = b"MOTEUR ROSE FACTICE - telecharge, pas embarque";
        let paquet = zip_stored("lib/net45/rose.exe", moteur_factice);
        let h = crate::sha256::sha256_hex(&paquet);
        let manifeste = format!(
            r#"[{{"PackageId":"Rose-0.1.6-full.nupkg","Version":"0.1.6","SHA256":"{h}","SHA1":"00","Size":"{}","Type":"Full"}}]"#,
            paquet.len()
        );
        let _ = fs::write(artefacts.join("releases.win.json"), &manifeste);
        let _ = fs::write(artefacts.join("Rose-0.1.6-full.nupkg"), &paquet);
        unsafe {
            std::env::set_var(
                "ROSE_LAUNCHER_BASE",
                format!("file://{}", racine.join("artefacts").display()),
            );
        }
        assert_eq!(telecharger_version("v0.1.6").unwrap(), "0.1.6");
        assert_eq!(
            fs::read(racine.join("app-0.1.6").join("rose.exe")).unwrap(),
            moteur_factice
        );
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.6"
        );
        // Le paquet téléchargé est supprimé après extraction.
        assert!(!racine.join("packages").join("Rose-0.1.6-full.nupkg").exists());

        // ── SHA-256 erroné : REFUS, aucune corruption de l'installation.
        let mauvais = manifeste.replace(&h, &"0".repeat(64));
        let _ = fs::write(artefacts.join("releases.win.json"), mauvais);
        let erreur = telecharger_version("v0.1.6").unwrap_err();
        assert!(erreur.contains("SHA-256 invalide"), "{erreur}");
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.6"
        );

        // ── API du sélecteur via les ponts `setup_core` ─────────────────
        // L'API factice (file://) annonce la release de test : le catalogue
        // est construit UNIQUEMENT depuis elle + l'état disque.
        let api_install = format!(
            r#"[{{"tag_name":"v0.1.6","name":"Rose v0.1.6",
                 "published_at":"2026-11-19T13:54:57Z",
                 "assets":[{{"name":"Rose-0.1.6-full.nupkg"}},{{"name":"releases.win.json"}}]}}]"#
        );
        let _ = fs::write(racine.join("api.json"), &api_install);
        let cat = crate::setup_core::launcher_catalogue();
        assert!(!cat.hors_ligne);
        // 3 entrées : la release distante v0.1.6 et les dossiers app-0.1.5 /
        // app-0.1.4 créés plus haut dans ce test (l'état local est fusionné).
        assert_eq!(cat.versions.len(), 3);
        assert_eq!(cat.versions[0].version, "0.1.6");
        assert_eq!(cat.versions[0].date, "2026-11-19T13:54:57Z");
        assert!(cat.versions[0].deja_installee);

        // Refus de sécurité : tag absent du catalogue, avant tout réseau.
        let refus = crate::setup_core::launcher_installer_version("v9.9.9");
        assert_eq!(refus.etat, EtatOperation::Refuse);
        assert!(refus.message.contains("absente du catalogue"));

        // Hash non vérifié : l'échec est remonté à l'UI avec un message.
        let echec = crate::setup_core::launcher_installer_version("v0.1.6");
        assert_eq!(echec.etat, EtatOperation::Echec);
        assert!(echec.message.contains("SHA-256 invalide"), "{}", echec.message);

        // Manifeste corrigé : installation réussie via le pont UI.
        let _ = fs::write(artefacts.join("releases.win.json"), &manifeste);
        let reussi = crate::setup_core::launcher_installer_version("v0.1.6");
        assert!(reussi.ok(), "{}", reussi.message);
        assert_eq!(reussi.version, "0.1.6");

        // Bascule sans réseau vers une version déjà sur disque.
        let selection = crate::setup_core::launcher_selectionner_version("v0.1.4");
        assert!(selection.ok(), "{}", selection.message);
        assert_eq!(
            fs::read_to_string(racine.join("current")).unwrap().trim(),
            "app-0.1.4"
        );

        // ── CACHE de la LISTE : données GitHub déjà reçues, avec date ──
        assert_eq!(dechapper_json(&echapper_json("a\"b\\c\nd\t")), "a\"b\\c\nd\t");
        assert_eq!(formater_date_cache(0), "1970-01-01 00:00 UTC");
        assert!(ecrire_cache_versions(API_FIXTURE));
        let cache = lire_cache_versions().expect("cache valide");
        assert_eq!(cache.versions, analyser_releases(API_FIXTURE));
        assert!(!cache_versions_perime(&cache), "cache frais");
        assert!(!crate::setup_core::launcher_cache_a_actualiser());
        let cat_cache = crate::setup_core::launcher_catalogue_cache().expect("catalogue cache");
        assert!(cat_cache.message.contains("Liste du"));
        assert!(!cat_cache.hors_ligne);
        // 3 versions distantes + la version 0.1.6 installée plus haut.
        assert_eq!(cat_cache.versions.len(), analyser_releases(API_FIXTURE).len() + 1);

        // Un cache vieux de plus du TTL est affiché MAIS signalé périmé.
        let vieux = format!(
            "{{\"horodatage\":\"{}\",\"brut\":\"{}\"}}",
            maintenant_unix().saturating_sub(TTL_CACHE_VERSIONS + 60),
            echapper_json(API_FIXTURE)
        );
        let _ = fs::write(racine.join(NOM_CACHE_VERSIONS), vieux);
        let cache_vieux = lire_cache_versions().expect("cache périmé lisible");
        assert!(cache_versions_perime(&cache_vieux));
        assert!(crate::setup_core::launcher_cache_a_actualiser());
        assert!(crate::setup_core::launcher_catalogue_cache()
            .expect("catalogue périmé").message.contains("périmée"));

        // Un cache corrompu n'invente RIEN : on repasse en mode sans cache.
        let _ = fs::write(
            racine.join(NOM_CACHE_VERSIONS),
            r#"{"horodatage":"1","brut":""}"#,
        );
        assert!(lire_cache_versions().is_none());
        assert!(crate::setup_core::launcher_catalogue_cache().is_none());

        unsafe {
            std::env::remove_var("ROSE_LAUNCHER_BASE");
            std::env::remove_var("ROSE_LAUNCHER_API");
            std::env::remove_var("ROSE_VELOPACK_ROOT");
        }
        let _ = fs::remove_dir_all(&racine);
    }

    /* ── MODE DE MISE À JOUR : parser, persistance, effet réel ───────── */

    #[test]
    fn parser_config_reconnait_les_trois_modes_ui_et_le_mode_historique() {
        // Les 3 choix des Options + le mode « manuel » historique (configs
        // déjà écrites avant la vague 72, toujours acceptées).
        assert_eq!(parser_config(r#"{"mode":"desactive"}"#).mode, ModeMiseAJour::Desactive);
        assert_eq!(parser_config(r#"{"mode":"auto"}"#).mode, ModeMiseAJour::Auto);
        assert_eq!(parser_config(r#"{"mode":"automatique_discret"}"#).mode, ModeMiseAJour::Automatique);
        assert_eq!(parser_config(r#"{"mode":"manuel"}"#).mode, ModeMiseAJour::Manuel);
        // alias tolérés
        assert_eq!(parser_config(r#"{"mode":"DÉSACTIVÉ"}"#).mode, ModeMiseAJour::Desactive);
        assert_eq!(parser_config(r#"{"mode":"MANUAL"}"#).mode, ModeMiseAJour::Manuel);
        assert_eq!(parser_config(r#"{"mode":"Automatic"}"#).mode, ModeMiseAJour::Auto);
        assert_eq!(parser_config(r#"{"mode":"Automatique (discret)"}"#).mode, ModeMiseAJour::Automatique);
        assert_eq!(parser_config(r#"{"mode":"discret"}"#).mode, ModeMiseAJour::Automatique);
        // Aller-retour exact de la valeur canonique, pour chaque mode.
        for mode in [ModeMiseAJour::Desactive, ModeMiseAJour::Auto,
                     ModeMiseAJour::Automatique, ModeMiseAJour::Manuel] {
            assert_eq!(ModeMiseAJour::depuis_str(mode.comme_str()), Some(mode));
            assert_eq!(parser_config(&format!(r#"{{"mode":"{}"}}"#, mode.comme_str())).mode, mode);
        }
        // Les 3 seules valeurs affichées, dans l'ordre, avec le bon libellé.
        let options = ModeMiseAJour::options_ui();
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].1, "Désactivée");
        assert_eq!(options[1].1, "À l'ouverture");
        assert_eq!(options[2].1, "Automatique (discret)");
        assert_eq!(options[0].0, MODE_PAR_DEFAUT);
        assert!(options[2].0.doit_demarrer_timer_discret());
        assert!(!options[0].0.doit_demarrer_timer_discret());
        assert!(!options[1].0.doit_demarrer_timer_discret());
        // Le mode historique se surligne comme « Désactivée ».
        assert_eq!(ModeMiseAJour::Manuel.pour_ui(), ModeMiseAJour::Desactive);
    }

    /// Régression : « À l'ouverture » (mode proposé par défaut) DOIT lancer le
    /// helper et s'enregistrer dans shell:startup, mais SANS timer 6 h.
    #[test]
    fn helper_lance_pour_les_deux_modes_automatiques_pas_pour_les_autres() {
        assert!(!ModeMiseAJour::Desactive.doit_lancer_helper());
        assert!(!ModeMiseAJour::Manuel.doit_lancer_helper());
        assert!(ModeMiseAJour::Auto.doit_lancer_helper());
        assert!(ModeMiseAJour::Automatique.doit_lancer_helper());
        // Le timer 6 h reste, lui, réservé au seul mode discret.
        assert!(!ModeMiseAJour::Auto.doit_demarrer_timer_discret());
        assert!(ModeMiseAJour::Automatique.doit_demarrer_timer_discret());
    }

    #[test]
    fn parser_config_valeurs_invalides_retombent_sur_le_defaut() {
        let cas = [
            "",
            "{}",
            "du texte",
            r#"{"mode":"inconnu"}"#,
            r#"{"mode":42}"#,
            r#"{"autre":"auto"}"#,
            r#"{"mode":null}"#,
        ];
        for json in cas {
            assert_eq!(
                parser_config(json).mode,
                MODE_PAR_DEFAUT,
                "json invalide {json:?} doit retomber sur le défaut"
            );
        }
        assert_eq!(MODE_PAR_DEFAUT, ModeMiseAJour::Desactive);
    }

    /// Un SEUL test pose `ROSE_CONFIG_DIR`/`ROSE_VELOPACK_URL` : les tests
    /// Rust tournant en parallèle, on ne touche PAS `ROSE_VELOPACK_ROOT`,
    /// déjà posé par `launcher_selection_et_cache` (mesuré : collision
    /// `LOCALAPPDATA indisponible` quand les deux tests le posent ensemble).
    #[test]
    fn config_persistance_et_effet_au_demarrage() {
        let racine = std::env::temp_dir().join(format!(
            "rose_config_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let dossier_cfg = racine.join("AppData").join("Roaming").join("Rose");
        let manifeste_absent = racine.join("manifeste-absent.json");
        // Édition 2024 : set_var est `unsafe`.
        unsafe {
            std::env::set_var("ROSE_CONFIG_DIR", &dossier_cfg);
            // Manifest intentionnellement absent : si une vérification est
            // tentée, `lire_source` renvoie une Erreur de lecture LOCALE
            // (aucun réseau) — preuve que l'appel a bien eu lieu.
            std::env::set_var(
                "ROSE_VELOPACK_URL",
                format!("file://{}", manifeste_absent.display()),
            );
        }
        let chemin = chemin_config();
        assert_eq!(chemin, dossier_cfg.join(NOM_CONFIG));
        assert!(!chemin.exists(), "aucun fichier au départ");

        // 1. Défaut : pas de fichier → Désactivée (config lue sans planter).
        //    Le mode discret est NOUVEAU et N'EST JAMAIS le défaut.
        assert_eq!(lire_config().mode, MODE_PAR_DEFAUT);
        assert_eq!(lire_config().mode, ModeMiseAJour::Desactive);
        assert_ne!(lire_config().mode, ModeMiseAJour::Automatique);

        // 2. Aller-retour pour CHAQUE mode : écrit → fichier lisible → relu.
        for mode in [ModeMiseAJour::Desactive, ModeMiseAJour::Auto,
                     ModeMiseAJour::Automatique, ModeMiseAJour::Manuel] {
            ecrire_config(mode).expect("écriture config");
            let brut = fs::read_to_string(&chemin).expect("config lisible");
            assert!(brut.contains(&format!("\"{}\": \"{}\"", CLE_MODE, mode.comme_str())), "{brut}");
            assert_eq!(lire_config().mode, mode, "aller-retour {mode:?}");
        }

        // 3. Écriture invalide → relecture tolérante sur le défaut.
        fs::write(&chemin, r#"{"mode":"bizarre"}"#).unwrap();
        assert_eq!(lire_config().mode, MODE_PAR_DEFAUT);

        // 4. Effet réel — mode MANUEL : AUCUNE vérification au démarrage,
        //    aucun appel réseau/fichier, donc Ok(None) même avec un manifeste
        //    absent.
        ecrire_config(ModeMiseAJour::Manuel).unwrap();
        assert!(!doit_verifier_au_demarrage());
        assert_eq!(verifier_au_demarrage().unwrap(), None);
        // Une vérification EXPLICITE, elle, est autorisée : elle échoue ici
        // uniquement parce que le manifeste n'existe pas (preuve de l'appel).
        let explicite = verifier_manifeste().unwrap_err();
        assert!(explicite.contains("lecture"), "vérification explicite attendue : {explicite}");
        // `rose update` (velopack) est autorisé en manuel : il échoue sur le
        // manifeste absent, PAS sur le mode.
        let update = check_and_update(true).unwrap_err();
        assert!(update.contains("lecture"), "rose update autorisé en manuel : {update}");

        // 5. Effet réel — mode DÉSACTIVÉ : tout est bloqué AVANT le manifeste,
        //    y compris `rose update`, avec le message explicite.
        ecrire_config(ModeMiseAJour::Desactive).unwrap();
        assert!(!doit_verifier_au_demarrage());
        assert_eq!(verifier_au_demarrage().unwrap(), None);
        let bloque = check_and_update(true).unwrap_err();
        assert!(bloque.contains("DÉSACTIVÉ"), "{bloque}");
        assert!(bloque.contains("rose update"), "{bloque}");
        let bloque_explicite = verifier_manifeste().unwrap_err();
        assert!(bloque_explicite.contains("DÉSACTIVÉ"), "{bloque_explicite}");

        // 6. Effet réel — mode AUTO (« À l'ouverture ») : la décision de
        //    démarrage est vraie et la vérification EST tentée (échec de
        //    lecture du manifeste absent) = la banderole L5 reste intacte.
        ecrire_config(ModeMiseAJour::Auto).unwrap();
        assert!(doit_verifier_au_demarrage());
        let auto = verifier_au_demarrage().unwrap_err();
        assert!(auto.contains("lecture"), "vérification auto attendue : {auto}");

        // 7. Effet réel — mode DISCRET : PAS de banderole à l'ouverture (le
        //    timer 6 h s'en charge), mais les actions explicites restent
        //    autorisées. Le tick himself est testé plus bas sans réseau.
        ecrire_config(ModeMiseAJour::Automatique).unwrap();
        assert!(!doit_verifier_au_demarrage(), "le discret ne doit PAS de banderole au démarrage");
        assert_eq!(verifier_au_demarrage().unwrap(), None);
        let discret_explicite = verifier_manifeste().unwrap_err();
        assert!(discret_explicite.contains("lecture"),
                "vérification explicite autorisée en discret : {discret_explicite}");

        unsafe {
            std::env::remove_var("ROSE_CONFIG_DIR");
            std::env::remove_var("ROSE_VELOPACK_URL");
        }
        let _ = fs::remove_dir_all(&racine);
    }

    /* ── L5 : banderole + manifeste site, SANS polling ───────────────── */

    /// Manifeste conforme au format annoncé par SITE1, avec un `changelog`
    /// qui contient le texte `"assets"` : le parseur ne doit PAS le confondre
    /// avec le vrai tableau d'assets.
    const SITE_FIXTURE: &str = r#"{
      "releases": [
        {"version":"0.1.7","tag":"v0.1.7","date":"2026-11-20T20:00:00Z",
         "titre":"Rose 0.1.7","changelog":"ajout de \"assets\" dans le texte",
         "assets":[{"nom":"Rose-0.1.7-full.nupkg","url":"https://exemple/Rose-0.1.7-full.nupkg","sha256":"abc"}]},
        {"version":"0.1.72","tag":"v0.1.72","date":"2026-11-21T09:00:00Z",
         "titre":"Correctifs","changelog":"correctifs",
         "assets":[{"nom":"Rose-0.1.72-full.nupkg","url":"https://exemple/Rose-0.1.72-full.nupkg","sha256":"def"}]},
        {"version":"0.1.3","tag":"v0.1.3","date":"2026-09-01T00:00:00Z",
         "titre":"Ancien","assets":[{"nom":"Rose-Setup-0.1.3.exe","url":"https://exemple/ancien.exe","sha256":"aaa"}]}
      ]
    }"#;

    #[test]
    fn manifeste_site_parse_liste_et_enveloppe() {
        let enveloppe = analyser_manifeste_site(SITE_FIXTURE).expect("manifeste valide");
        // Tri DÉCROISSANT : 0.1.72 > 0.1.7 > 0.1.3 (comparer_versions).
        assert_eq!(
            enveloppe.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            vec!["0.1.72", "0.1.7", "0.1.3"]
        );
        assert_eq!(enveloppe[0].tag, "v0.1.72");
        assert_eq!(enveloppe[0].date, "2026-11-21T09:00:00Z");
        assert_eq!(enveloppe[0].titre, "Correctifs");
        assert_eq!(enveloppe[0].nupkg, "Rose-0.1.72-full.nupkg");
        assert!(enveloppe[0].installable, "SHA-256 annoncé");
        assert!(!enveloppe[2].installable, "ancien installeur legacy");
        assert_eq!(enveloppe[2].legacy, "Rose-Setup-0.1.3.exe");

        // Liste nue + clés anglaises tolérées.
        let nu = r#"[{"version":"0.2.0","title":"Deux","date":"2026-11-21",
            "assets":[{"name":"Rose-0.2.0-full.nupkg","sha256":"11"}]},
            {"version":"0.2.1","assets":[{"name":"Rose-0.2.1-full.nupkg"}]}]"#;
        let versions = analyser_manifeste_site(nu).expect("liste nue valide");
        assert_eq!(versions[0].version, "0.2.1");
        assert!(!versions[0].installable, "SHA-256 absent = jamais installable");
        assert_eq!(versions[1].titre, "Deux");

        // Format RÉEL de docs/telechargements/releases.json : clé "versions".
        let reel = r#"{"source":"site","genere":"2026-11-27","versions":[
            {"version":"0.1.86","tag":"v0.1.86","date":"2026-11-27",
             "titre":"Rose v0.1.86","assets":[{"nom":"Rose-0.1.86-full.nupkg",
             "url":"https://stef302.github.io/Rose/telechargements/Rose-0.1.86-full.nupkg",
             "sha256":"b4906c252a161aac54d0f6bc4b73aae723e699805d84c1d34e5fb19c71c4ca56"}]}]}"#;
        let versions = analyser_manifeste_site(reel).expect("enveloppe 'versions' du site");
        assert_eq!(versions[0].version, "0.1.86", "le manifeste réel doit être lu");
        assert!(versions[0].installable, "SHA-256 réel annoncé");
        assert_eq!(versions[0].nupkg, "Rose-0.1.86-full.nupkg");
    }

    #[test]
    fn manifeste_site_invalide_est_rejete_sans_inventer() {
        for json in [
            "",
            "[]",
            r#"{"autre":[]}"#,
            r#"[{"version":" Rose nouvelle","assets":[]}]"#,
            r#"{"releases":[{"version":""}]}"#,
        ] {
            assert!(analyser_manifeste_site(json).is_err(), "JSON invalide accepté : {json}");
        }
    }

    #[test]
    fn bandeau_ne_ment_jamais() {
        let versions = analyser_manifeste_site(SITE_FIXTURE).unwrap();
        let b = bandeau_depuis_versions("0.1.7", &versions).expect("0.1.72 plus récente");
        assert_eq!(b.texte(), "Nouvelle version disponible : 0.1.7 → 0.1.72");
        assert!(bandeau_depuis_versions("v0.1.7", &versions).is_some());
        assert!(bandeau_depuis_versions("0.1.72", &versions).is_none(), "déjà à jour");
        assert!(bandeau_depuis_versions("0.1.73", &versions).is_none(), "version inconnue plus haute");
        assert!(bandeau_depuis_versions("", &versions).is_none(), "aucune version installée connue");
        assert!(bandeau_depuis_versions("inconnue", &versions).is_none());
        let simples = vec!["0.1.7".to_string(), "0.1.72".to_string()];
        assert!(bandeau_depuis_chaines("0.1.7", &simples).is_some());
    }

    #[test]
    fn cache_site_utilise_le_meme_mecanisme_et_le_meme_ttl() {
        let racine = std::env::temp_dir().join(format!(
            "rose_cache_site_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_nanos()
        ));
        let _ = fs::create_dir_all(&racine);
        assert!(ecrire_cache_brut_dans(&racine, NOM_CACHE_SITE, SITE_FIXTURE));
        let (ts, brut) = lire_cache_brut_dans(&racine, NOM_CACHE_SITE).expect("cache lisible");
        assert_eq!(brut, SITE_FIXTURE);
        assert!(!cache_perime(ts), "cache frais");
        assert!(cache_frais(ts));
        assert!(cache_perime(ts.saturating_sub(TTL_CACHE_VERSIONS + 1)));
        let versions = analyser_manifeste_site(&brut).unwrap();
        assert_eq!(versions[0].version, "0.1.72");
        // Forcer réécrit le même fichier (le bouton « Rafraîchir »).
        assert!(ecrire_cache_brut_dans(&racine, NOM_CACHE_SITE, SITE_FIXTURE));
        assert!(lire_cache_brut_dans(&racine, NOM_CACHE_SITE).is_some());
        let _ = fs::remove_dir_all(&racine);
    }

    /// Preuve STRUCTURELLE d'absence de polling : la fonction lancée au
    /// démarrage contient exactement UN `spawn`, aucune boucle, aucun sleep,
    /// aucun timer Windows. Un thread unique qui meurt après une réponse.
    #[test]
    fn verification_au_demarrage_sans_polling() {
        let src = include_str!("setup_gui.rs");
        let debut = src.find("fn lancer_verification_maj").expect("fonction présente");
        let fin = src.find("/// Consentement explicite").expect("fin de fonction");
        let bloc = &src[debut..fin];
        assert_eq!(bloc.matches("std::thread::spawn").count(), 1);
        for interdit in ["loop {", "sleep(", "Sleep(", "SetTimer", "while true"] {
            assert!(!bloc.contains(interdit), "mécanisme de polling détecté : {interdit}");
        }
        assert!(bloc.contains("verifier_bandeau(explicite)"), "source site obligatoire");
    }

    /// Mesure (« 5 tirs ») du temps jusqu'à la banderole, cache FROID puis
    /// cache CHAUD. Le manifeste est lu depuis un `file://` local : la partie
    /// réseau réelle est bornée à `DELAI_RESEAU_MS` (3 s) par `requeter_json`.
    /// Aucune variable d'environnement n'est touchée (tests parallèles).
    #[test]
    fn mesure_bandeau_cache_froid_et_chaud() {
        let racine = std::env::temp_dir().join(format!(
            "rose_mesure_bandeau_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_nanos()
        ));
        let _ = fs::create_dir_all(&racine);
        let fixture = racine.join("releases.json");
        fs::write(&fixture, SITE_FIXTURE).unwrap();
        let url = format!("file://{}", fixture.display());
        let mesurer = |froid: bool| -> Vec<u128> {
            let mut tirs = Vec::new();
            for _ in 0..5 {
                if froid {
                    let _ = fs::remove_file(racine.join(NOM_CACHE_SITE));
                }
                let debut = Instant::now();
                let r = lister_versions_site_depuis(&url, Some(&racine), false)
                    .expect("manifeste local lisible");
                let bandeau = bandeau_depuis_versions("0.1.7", &r.versions);
                tirs.push(debut.elapsed().as_micros());
                assert!(bandeau.is_some());
            }
            tirs.sort_unstable();
            tirs
        };
        let froid = mesurer(true);
        let chaud = mesurer(false);
        eprintln!("BANDEAU froid (5 tirs, µs) : {froid:?} — médiane {}", froid[2]);
        eprintln!("BANDEAU chaud (5 tirs, µs) : {chaud:?} — médiane {}", chaud[2]);
        assert_eq!(froid.len(), 5);
        assert_eq!(chaud.len(), 5);
        let _ = fs::remove_dir_all(&racine);
    }

    /* ── L6 : timer discret — logique pure, sans Windows ni réseau ────── */

    /// L'intervalle du timer est LONG (≥ 6 h) et tient dans le `u32` de
    /// `SetTimer`. Aucun polling court n'est possible par construction.
    #[test]
    fn timer_discret_intervalle_long_et_borne() {
        assert_eq!(INTERVALLE_TIMER_DISCRET_MS, 21_600_000);
        assert!(intervalle_timer_discret_valide(INTERVALLE_TIMER_DISCRET_MS));
        assert!(intervalle_timer_discret_valide(21_600_000));
        assert!(!intervalle_timer_discret_valide(21_599_999)); // 5 h 59 m 59 s
        assert!(!intervalle_timer_discret_valide(60_000)); // 1 minute = interdit
        assert!(intervalle_timer_discret_valide(u32::MAX));
        assert!(!REPLI_GITHUB_DANS_TIMER_DISCRET, "un tick = UNE requête site maximum");
    }

    /// Le garde-fou de mode est testé sans variable d'environnement : hors du
    /// mode discret, aucun tick ne lit le disque ni le réseau.
    #[test]
    fn timer_discret_garde_fou_de_mode() {
        for mode in [ModeMiseAJour::Desactive, ModeMiseAJour::Auto, ModeMiseAJour::Manuel] {
            assert!(!mode.doit_demarrer_timer_discret(), "mode {mode:?} ne doit pas poser de timer");
            assert_eq!(verification_discrete_pour_mode(mode).unwrap(), None,
                       "mode {mode:?} : aucun appel réseau attendu");
        }
        assert!(ModeMiseAJour::Automatique.doit_demarrer_timer_discret());
    }

    /// Un tick = UNE lecture du manifeste SITE (cache ou HTTP 3 s) + comparaison.
    /// Le second tick est servi par le cache : zéro requête réseau supplémentaire.
    #[test]
    fn timer_discret_une_lecture_site_puis_cache() {
        let racine = std::env::temp_dir().join(format!(
            "rose_timer_discret_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_nanos()
        ));
        let _ = fs::create_dir_all(&racine);
        let fixture = racine.join("releases.json");
        fs::write(&fixture, SITE_FIXTURE).unwrap();
        let url = format!("file://{}", fixture.display());

        let premier = verification_discrete_depuis(&url, Some(&racine), "0.1.7".into())
            .expect("manifeste local lisible")
            .expect("0.1.72 > 0.1.7 doit être annoncée");
        // La source peut être Site (1re lecture) ou CacheSite (cache persistant
        // d'une salve précédente) : ce qui compte est la版本 annoncée.
        assert!(matches!(premier.source, SourceVersions::Site | SourceVersions::CacheSite));
        assert_eq!(premier.disponible, "0.1.72");
        assert!(doit_notifier_discret(Some(&premier)), "pastille discrète attendue");

        let second = verification_discrete_depuis(&url, Some(&racine), "0.1.7".into())
            .expect("cache local lisible")
            .expect("même version annoncée depuis le cache");
        assert_eq!(second.source, SourceVersions::CacheSite,
                   "le 2e tick ne refait PAS de requête HTTP");
        assert_eq!(second.disponible, premier.disponible);
        assert_eq!(second.installee, premier.installee);

        // À jour : aucune pastille, même avec un manifeste parfaitement lisible.
        let a_jour = verification_discrete_depuis(&url, Some(&racine), "0.1.72".into())
            .expect("cache lisible");
        assert!(a_jour.is_none());
        assert!(!doit_notifier_discret(a_jour.as_ref()));

        let _ = fs::remove_dir_all(&racine);
    }

    /// Preuve STRUCTURELLE du « UNE requête HTTP par tick » : le chemin du
    /// tick contient UN seul appel `requeter_json` et aucun repli GitHub.
    #[test]
    fn timer_discret_structure_une_seule_requete() {
        let src = include_str!("velopack.rs");
        let debut = src.find("fn lister_versions_site_depuis(").expect("lecture site présente");
        let fin = src.find("/// Source de vérité de toute l'UI").expect("fin lecture site");
        let bloc = &src[debut..fin];
        assert_eq!(bloc.matches("requeter_json(").count(), 1,
                   "UNE seule requête HTTP pour lire le manifeste site");
        assert!(!bloc.contains("check_and_update"), "le tick ne télécharge jamais");

        let debut2 = src.find("fn verification_discrete_depuis(").expect("tick présent");
        let fin2 = src.find("/* ── Catalogue du sélecteur").expect("fin tick");
        let tick = &src[debut2..fin2];
        assert!(!tick.contains("lister_versions_source"),
                "aucun repli GitHub dans le tick (sinon 2 requêtes)");
        assert!(!tick.contains("lister_versions()"), "aucun appel GitHub dans le tick");
        assert!(tick.contains("lister_versions_site_depuis"), "source = site Rose");
        assert_eq!(tick.matches("requeter_json(").count(), 0,
                   "le tick délègue l'unique requête à la lecture site");
    }

    /// Preuve STRUCTURELLE du « thread unique endormi, zéro CPU entre ticks » :
    /// le code du timer ne contient qu'UN spawn, aucune boucle, aucun sleep,
    /// et un seul SetTimer à intervalle long, tué quand on quitte le mode.
    #[test]
    fn timer_discret_structure_sans_polling() {
        let src = include_str!("setup_gui.rs");
        let debut = src.find("fn lancer_verification_discrete").expect("fonction présente");
        let fin = src.find("fn dans(").expect("fin de bloc");
        let bloc = &src[debut..fin];
        assert_eq!(bloc.matches("std::thread::spawn").count(), 1);
        for interdit in ["loop {", "sleep(", "Sleep(", "while true", "SetTimer"] {
            assert!(!bloc.contains(interdit), "polling détecté dans le tick : {interdit}");
        }
        assert!(bloc.contains("verification_discrete()"), "le tick lit le manifeste SITE");
        assert!(src.contains("INTERVALLE_TIMER_DISCRET_MS"), "intervalle long câblé");
        // Le timer est identifié par sa constante TIMER_MAJ_DISCRETE : un seul
        // SetTimer et un seul KillTimer (mesurés ci-dessous). L'ancien test
        // exigeait 2 occurrences de la constante au total, ce qui était trop
        // strict dès qu'un seul des deux appels est présent.
        assert!(src.matches("TIMER_MAJ_DISCRETE").count() >= 2,
                "timer discret référencé au moins par SetTimer et KillTimer");
        assert_eq!(src.matches("SetTimer(h, TIMER_MAJ_DISCRETE").count(), 1);
        assert_eq!(src.matches("KillTimer(h, TIMER_MAJ_DISCRETE").count(), 1);
    }

    /* ── DÉCOUVERTE DYNAMIQUE : le lanceur voit les releases SANS nupkg ── */

    /// Reproduction fidèle des releases v0.1.77 → v0.1.79 : quatre `.exe`
    /// génériques, AUCUN `.nupkg`. L'ancien analyseur les écartait toutes.
    const API_GENERIQUE_FIXTURE: &str = r#"[
      {"author":{"login":"Stef302"},"tag_name":"v0.1.79",
       "published_at":"2026-11-26T09:42:06Z",
       "name":"Rose v0.1.79 — self-hosté & optimisé",
       "body":"Rose-win-Setup.exe est mentionné dans le texte aussi",
       "assets":[{"name":"rose.exe"},{"name":"Rose-win-Setup.exe"},
                 {"name":"rose-shell.exe"},{"name":"rose-tray.exe"}]},
      {"tag_name":"v0.1.78","name":"Rose v0.1.78",
       "assets":[{"name":"Rose-win-Setup.exe"},{"name":"rose.exe"}]},
      {"tag_name":"v0.1.76","name":"Rose v0.1.76",
       "assets":[{"name":"releases.win.json"},{"name":"Rose-0.1.76-full.nupkg"}]}
    ]"#;

    #[test]
    fn decouverte_voit_releases_sans_nupkg() {
        let versions = analyser_releases(API_GENERIQUE_FIXTURE);
        assert_eq!(versions.len(), 3, "les 3 releases Windows doivent être annoncées");
        assert_eq!(versions[0].version, "0.1.79");
        assert_eq!(versions[0].tag, "v0.1.79");
        assert_eq!(versions[0].date, "2026-11-26T09:42:06Z");
        assert_eq!(versions[0].titre, "Rose v0.1.79 — self-hosté & optimisé");
        assert_eq!(versions[0].legacy, "Rose-win-Setup.exe");
        assert!(!versions[0].installable, "pas de nupkg : annoncée mais non installable par packages\\");
        assert!(versions[2].installable, "v0.1.76 garde son paquet complet");
        assert_eq!(versions[2].nupkg, "Rose-0.1.76-full.nupkg");
    }

    #[test]
    fn decouverte_trie_et_ignore_releases_sans_artefact_windows() {
        let json = r#"[
          {"tag_name":"v9.9.9","name":"Sans artefact","assets":[{"name":"notes.md"}]},
          {"tag_name":"v0.1.77","name":"A","assets":[{"name":"Rose-win-Setup.exe"}]},
          {"tag_name":"v0.1.79","name":"C","assets":[{"name":"rose.exe"},
                                                    {"name":"Rose-win-Setup.exe"}]}
        ]"#;
        let versions = analyser_releases(json);
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "0.1.79");
        assert!(versions.iter().all(|v| v.tag != "v9.9.9"));
    }

    #[test]
    fn fallback_manifeste_local_convertit_les_paquets() {
        let manifeste = Manifeste::analyser(
            r#"[
              {"PackageId":"Rose-0.1.5-full.nupkg","Version":"0.1.5",
               "SHA256":"aa","SHA1":"11","Size":"100","Type":"Full"},
              {"PackageId":"Rose-0.1.5-delta.nupkg","Version":"0.1.5",
               "SHA256":"bb","SHA1":"22","Size":"50","Type":"Delta"}
            ]"#,
        )
        .expect("manifeste lisible");
        let versions = versions_depuis_manifeste(&manifeste);
        assert_eq!(versions.len(), 1, "le delta est écarté");
        assert_eq!(versions[0].tag, "v0.1.5");
        assert_eq!(versions[0].nupkg, "Rose-0.1.5-full.nupkg");
        assert!(versions[0].installable, "SHA-256 présent");
    }

    /// Preuve STRUCTURELLE de l'ordre réel dans `decouvrir_versions_update` :
    /// surcharges explicites → SITE → GitHub → manifeste local.
    #[test]
    fn decouverte_update_ordre_site_puis_github_puis_local() {
        let src = include_str!("velopack.rs");
        let debut = src.find("pub fn decouvrir_versions_update()").expect("fonction présente");
        let fin = src.find("/// URLs du manifeste ET du paquet").expect("fin découverte");
        let bloc = &src[debut..fin];
        let surcharge = bloc.find("ROSE_VELOPACK_URL").expect("surcharge 1");
        let diagnostic = bloc.find("ROSE_LAUNCHER_API").expect("surcharge 2");
        let site = bloc.find("lister_versions_site_depuis").expect("site en premier");
        let github = bloc.find("lister_versions()").expect("repli GitHub");
        let local = bloc.find("charger_manifeste_local()").expect("repli local");
        assert!(surcharge < diagnostic && diagnostic < site, "surcharges AVANT le site");
        assert!(site < github, "le SITE doit être interrogé AVANT GitHub");
        assert!(github < local, "GitHub avant le manifeste local");
    }

    /// Preuve FONCTIONNELLE hors ligne : `decouvrir_versions_update` lit le
    /// manifeste du SITE en premier et rend sa dernière version, même quand
    /// une version n'existe PAS sur GitHub (0.1.99 ici). `racine = None`
    /// écarte tout cache : la lecture vient bien du `file://` fourni.
    #[test]
    fn decouverte_update_site_premier_hors_ligne() {
        let racine = std::env::temp_dir().join(format!(
            "rose_decouverte_site_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_nanos()
        ));
        let _ = fs::create_dir_all(&racine);
        let manifeste = racine.join("releases.json");
        fs::write(&manifeste, r#"{"source":"site","genere":"2026-11-27","versions":[
          {"version":"0.1.7","tag":"v0.1.7","titre":"Ancienne",
           "assets":[{"nom":"Rose-0.1.7-full.nupkg","sha256":"aa"}]},
          {"version":"0.1.99","tag":"v0.1.99","titre":"Site seulement",
           "assets":[{"nom":"Rose-0.1.99-full.nupkg","sha256":"bb"}]}
        ]}"#).expect("manifeste écrit");
        let url = format!("file://{}", manifeste.display());
        let resultat = decouvrir_versions_update_depuis(Some(&url), None, true)
            .expect("découverte depuis le site");
        assert_eq!(resultat.source, SourceVersions::Site, "source réelle = site Rose");
        assert_eq!(resultat.versions[0].version, "0.1.99",
                   "aucune release GitHub 0.1.99 : la liste vient donc du site");
        assert!(resultat.versions[0].installable);
        let _ = fs::remove_dir_all(&racine);
    }

    /// Preuve RÉELLE, en lecture seule, que `rose update` voit les releases
    /// annoncées par la source primaire (le site, GitHub en repli).
    /// Commande : `cargo test --release decouverte_update_source_reelle -- --ignored --nocapture`
    #[test]
    #[ignore = "accès réel au site / api.github.com, lecture seule"]
    fn decouverte_update_source_reelle() {
        let decouverte = decouvrir_versions_update().expect("site ou repli GitHub accessible");
        assert_ne!(decouverte.source, SourceVersions::Local, "le réseau doit répondre");
        assert!(decouverte.versions.iter().any(|v| v.version == "0.1.86"),
                "versions vues : {:?}",
                decouverte.versions.iter().map(|v| v.version.as_str()).collect::<Vec<_>>());
        let (derniere, source) = decouvrir_derniere_version("0.1.76").expect("découverte");
        let d = derniere.expect("0.1.86 est strictement supérieure à 0.1.76");
        assert_eq!(d.version, "0.1.86");
        assert_eq!(d.tag, "v0.1.86");
        println!("ROSE UPDATE VOIT : 0.1.76 → {} (source {})", d.version, source.libelle());
    }
}
