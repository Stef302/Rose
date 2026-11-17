// ═══════════════════════════════════════════════════════════════════════
// WASM — RUNTIME WEBASSEMBLY OFFICIEL OPTIONNEL, À LA DEMANDE
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI CE MODULE (plan M6.2). M6.1 a livré un pont WebAssembly
// maison (`wasm_lecteur`) : un interpréteur de pile, ZÉRO dépendance, qui
// reste LE DÉFAUT. Ce module ajoute simplement le CHOIX pour les modules
// chauds : un runtime officiel (wasmtime / wasmer) peut être installé dans
// le cache utilisateur, puis utilisé à la place de l'interpréteur.
//
// RÈGLES D'OR (contractuelles, voir docs/WASM_RUNTIME.md) :
//
//   1. l'interpréteur maison est TOUJOURS le défaut ; rien n'est embarqué ;
//   2. le runtime officiel est un EXÉCUTABLE SÉPARÉ, rangé dans
//      `~/.cache/rose/runtimes/` sous un nom déterministe
//      (runtime + version) — jamais lié dans `rose.exe` ;
//   3. le téléchargement est EXPLICITE (`installer()`), avec vérification
//      SHA-256 OBLIGATOIRE. `executer_export()` ne télécharge JAMAIS :
//      si le runtime est absent, il replie sur l'interpréteur maison en
//      le disant (jamais de téléchargement silencieux pendant une requête) ;
//   4. aucune dépendance Rust ajoutée : le téléchargement passe par `curl`
//      (ou `wget`) via `std::process::Command`, exactement comme `auto_llvm`
//      le fait pour le helper AOT.
//
// ⚠ Le runner officiel est un petit exécutable (-fourni par une release
// Stef302/Rose-) qui respecte le contrat :
//
//       runner <module.wasm> <export> [arg1 arg2 ...]
//
//   il écrit UN ENTIER SIGNÉ DÉCIMAL PAR LIGNE sur stdout (un résultat par
//   valeur de retour de l'export). C'est ce format, et lui seul, qui rend la
//   sortie IDENTIQUE à celle de l'interpréteur maison. Un exemple de wrapper
//   autour de `wasmtime run --invoke` est donné dans la documentation.
//
// Licence MIT.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Version du runner officiel (doit correspondre au TAG de release Rose).
/// À incrémenter quand un nouveau runner est publié.
pub const VERSION_RUNTIME: &str = "v0.1.5";

/// Sous-dossier du cache : `~/.cache/rose/runtimes/`.
pub const SOUS_DOSSIER_CACHE: &str = "runtimes";

/// ═══ UN RUNTIME OFFICIEL PRIS EN CHARGE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeOfficiel {
    Wasmtime,
    Wasmer,
}

impl RuntimeOfficiel {
    /// Nom canonique (utilisé dans le nom de fichier et les messages).
    pub fn nom(self) -> &'static str {
        match self { RuntimeOfficiel::Wasmtime => "wasmtime", RuntimeOfficiel::Wasmer => "wasmer" }
    }

    /// Analyse une valeur de CLI : `officiel`/`official` = wasmtime par défaut.
    pub fn depuis_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "officiel" | "official" | "wasmtime" | "wt" => Some(RuntimeOfficiel::Wasmtime),
            "wasmer" | "wm" => Some(RuntimeOfficiel::Wasmer),
            _ => None,
        }
    }
}

/// ═══ VOIE D'EXÉCUTION EFFECTIVEMENT EMPLOYÉE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voie {
    /// Interpréteur de pile maison (défaut, zéro dépendance).
    Maison,
    /// Runner officiel installé dans le cache.
    Officiel,
}

/// ═══ RÉSULTAT D'UN APPEL D'EXPORT WASM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resultat {
    /// Voie réellement utilisée.
    pub voie: Voie,
    /// Valeurs de retour, normalisées en entiers signés (format identique
    /// quelle que soit la voie : c'est ce qui garantit « sortie identique »).
    pub valeurs: Vec<i64>,
    /// Raison d'un repli sur l'interpréteur maison (`None` si aucun repli).
    /// Portée à la connaissance de l'appelant : jamais un silence.
    pub repli: Option<String>,
}

/// ═══ UNE ERREUR DE CE MODULE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurRuntime(pub String);

impl std::fmt::Display for ErreurRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ErreurRuntime {}

fn err<T>(m: impl Into<String>) -> Result<T, ErreurRuntime> { Err(ErreurRuntime(m.into())) }

// ═══════════════════════════════════════════════════════════════════════
// CACHE — CHEMINS DÉTERMINISTES
// ═══════════════════════════════════════════════════════════════════════

/// Dossier des runtimes : `~/.cache/rose/runtimes/`.
///
/// Surcharges (ordre de priorité) : `ROSE_CACHE` → `XDG_CACHE_HOME` → `$HOME`.
/// Même convention que `auto_llvm::dossier_cache()`, avec le sous-dossier
/// `runtimes/` en plus pour ne pas mélanger les artefacts.
pub fn dossier_cache() -> PathBuf {
    if let Ok(v) = std::env::var("ROSE_CACHE") {
        return PathBuf::from(v).join(SOUS_DOSSIER_CACHE);
    }
    let base = std::env::var("XDG_CACHE_HOME").ok().map(PathBuf::from)
        .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("rose").join(SOUS_DOSSIER_CACHE)
}

/// Nom de fichier DÉTERMINISTE : `<runtime>-<version>` (extension `.exe`
/// sous Windows). Exemple : `wasmtime-v0.1.5`.
pub fn nom_fichier(rt: RuntimeOfficiel) -> String {
    let base = format!("{}-{}", rt.nom(), VERSION_RUNTIME);
    if cfg!(windows) { format!("{base}.exe") } else { base }
}

/// Nom de l'artefact de release (différent du nom de cache : il contient
/// la cible, comme le helper AOT).
pub fn nom_artefact(rt: RuntimeOfficiel) -> String {
    format!("rose-{}-runner-{}-{}", rt.nom(), VERSION_RUNTIME, cible())
}

/// Plateforme cible, pour l'URL de l'artefact.
pub fn cible() -> String {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "linux") => "aarch64-linux".to_string(),
        ("x86_64", "linux") => "x86_64-linux".to_string(),
        ("x86_64", "windows") => "x86_64-windows.exe".to_string(),
        ("aarch64", "macos") => "aarch64-macos".to_string(),
        ("x86_64", "macos") => "x86_64-macos".to_string(),
        (a, o) => format!("{a}-{o}"),
    }
}

/// Chemin complet du runner dans le cache.
pub fn chemin_runtime(rt: RuntimeOfficiel) -> PathBuf {
    dossier_cache().join(nom_fichier(rt))
}

/// URL par défaut de l'artefact (release GitHub **Stef302/Rose**).
pub fn url_runtime(rt: RuntimeOfficiel) -> String {
    format!(
        "https://github.com/Stef302/Rose/releases/download/{}/{}",
        VERSION_RUNTIME,
        nom_artefact(rt)
    )
}

/// Empreinte SHA-256 attendue. `ROSE_WASM_RUNTIME_SHA256` permet de
/// surcharger (miroir interne, tests, déploiement privé).
///
/// ⚠ Tant qu'un runner officiel n'a PAS été publié, la table intégrée est
/// vide : `installer()` refuse alors de télécharger (on n'exécute jamais un
/// binaire non authentifié). Renseigner la constante quand l'artefact est
/// publié, ou définir la variable d'environnement.
pub fn sha256_attendu(rt: RuntimeOfficiel) -> String {
    if let Ok(v) = std::env::var("ROSE_WASM_RUNTIME_SHA256") {
        return v;
    }
    let _ = rt; // table par runtime à renseigner lors de la publication
    String::new()
}

/// Vérifie des octets contre une empreinte SHA-256 (hexadécimale).
///
/// Réutilise `crate::sha256::sha256_hex` : AUCUNE duplication de code.
pub fn verifier_sha256(octets: &[u8], attendu: &str) -> Result<(), ErreurRuntime> {
    let attendu = attendu.trim().to_ascii_lowercase();
    if attendu.is_empty() {
        return err("aucune empreinte SHA-256 de référence (définir ROSE_WASM_RUNTIME_SHA256)");
    }
    let obtenu = crate::sha256::sha256_hex(octets);
    if obtenu != attendu {
        return err(format!("empreinte SHA-256 inattendue (obtenu {obtenu}, attendu {attendu})"));
    }
    Ok(())
}

#[cfg(unix)]
fn est_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}
#[cfg(not(unix))]
fn est_executable(p: &Path) -> bool { p.is_file() }

/// Le runtime est-il déjà installé (cache ou PATH) ?
pub fn est_installe(rt: RuntimeOfficiel) -> bool { trouver_runtime(rt).is_some() }

/// Cherche un runner installé : cache d'abord, puis PATH.
pub fn trouver_runtime(rt: RuntimeOfficiel) -> Option<PathBuf> {
    let p = chemin_runtime(rt);
    if est_executable(&p) { return Some(p); }
    // Nom court dans le PATH (utile pour un runner installé par le système).
    let court = format!("rose-{}-runner", rt.nom());
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if dir.is_empty() { continue; }
            let p = Path::new(dir).join(&court);
            if est_executable(&p) { return Some(p); }
        }
    }
    None
}

/// ═══ INSTALLATION EXPLICITE (téléchargement + SHA-256 + cache).
///
/// Cette fonction est le SEUL endroit d'où part un téléchargement. Elle doit
/// être appelée par une commande dédiée (`rose wasm --install-runtime …`),
/// jamais par le chemin d'exécution normal.
#[cfg(not(target_arch = "wasm32"))]
pub fn installer(rt: RuntimeOfficiel) -> Result<PathBuf, ErreurRuntime> {
    let url = std::env::var("ROSE_WASM_RUNTIME_URL")
        .unwrap_or_else(|_| url_runtime(rt));
    let attendu = sha256_attendu(rt);
    // Refus immédiat : on n'exécute pas un binaire téléchargé non authentifié.
    if attendu.trim().is_empty() {
        return err(format!(
            "aucune empreinte SHA-256 pour '{}' : définir ROSE_WASM_RUNTIME_SHA256 \
             (ou publier l'artefact et renseigner la table intégrée)",
            rt.nom()
        ));
    }

    let cache = dossier_cache();
    std::fs::create_dir_all(&cache)
        .map_err(|e| ErreurRuntime(format!("création du cache : {e}")))?;
    let tmp = cache.join(format!(".{}.tmp", nom_fichier(rt)));

    if !downloadable(&url, &tmp)? {
        let _ = std::fs::remove_file(&tmp);
        return err(format!("téléchargement impossible : {url} (curl/wget absent ou URL inaccessible)"));
    }

    let octets = std::fs::read(&tmp)
        .map_err(|e| ErreurRuntime(format!("lecture du téléchargé : {e}")))?;
    if let Err(e) = verifier_sha256(&octets, &attendu) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    let cible = chemin_runtime(rt);
    let _ = std::fs::remove_file(&cible);
    std::fs::rename(&tmp, &cible)
        .map_err(|e| ErreurRuntime(format!("installation dans le cache : {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&cible)
            .map_err(|e| ErreurRuntime(format!("métadonnées : {e}")))?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&cible, perm)
            .map_err(|e| ErreurRuntime(format!("permissions : {e}")))?;
    }
    Ok(cible)
}

/// Télécharge `url` vers `dest` via curl (puis wget). Zéro dépendance Rust.
/// Limite : exige un outil en ligne de commande et une connexion réseau ;
/// aucun proxy/TLS n'est géré en propre (curl/wget s'en chargent).
#[cfg(not(target_arch = "wasm32"))]
fn downloadable(url: &str, dest: &Path) -> Result<bool, ErreurRuntime> {
    use std::process::Command;

    if which("curl").is_some() {
        let st = Command::new("curl")
            .args(["-fsSL", "--retry", "2", "-o"])
            .arg(dest)
            .arg(url)
            .status();
        if let Ok(st) = st {
            if st.success() { return Ok(true); }
        }
    }
    if which("wget").is_some() {
        let st = Command::new("wget")
            .args(["-q", "-O"])
            .arg(dest)
            .arg(url)
            .status();
        if let Ok(st) = st {
            if st.success() { return Ok(true); }
        }
    }
    Ok(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn which(prog: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if dir.is_empty() { continue; }
            let p = Path::new(dir).join(prog);
            if est_executable(&p) { return Some(p); }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════
// EXÉCUTION
// ═══════════════════════════════════════════════════════════════════════

/// Exécute un export wasm avec l'interpréteur MAISON (défaut, zéro dépendance).
///
/// ⚠⚠ LES ARGUMENTS DOIVENT ÊTRE TYPÉS SELON LA SIGNATURE DE L'EXPORT :
/// `ValWasm::as_i32()` ne convertit PAS un `I64` (il rend 0 pour toute autre
/// variante), donc passer systématiquement `ValWasm::I64(27)` faisait rendre
/// 0 à `fib(27)` — mesuré. On lit donc `module.fonctions[idx].params` et on
/// construit chaque `ValWasm` dans le bon variant.
pub fn executer_maison(
    chemin_wasm: &Path,
    export: &str,
    arguments: &[i64],
) -> Result<Vec<i64>, ErreurRuntime> {
    use crate::wasm_lecteur::{TyWasm, ValWasm};

    let octets = std::fs::read(chemin_wasm).map_err(|e| {
        ErreurRuntime(format!("lecture de {} : {e}", chemin_wasm.display()))
    })?;
    let module = crate::wasm_lecteur::charger(&octets)
        .map_err(|e| ErreurRuntime(format!("module wasm invalide : {e}")))?;
    let module = std::sync::Arc::new(module);
    let idx = module.exports.iter()
        .find(|(n, genre, _)| n == export && *genre == 0)
        .map(|(_, _, i)| *i)
        .ok_or_else(|| ErreurRuntime(format!("fonction exportée '{export}' introuvable")))?;
    let params = module.fonctions.get(idx as usize)
        .map(|f| f.params.clone())
        .ok_or_else(|| ErreurRuntime(format!("fonction {idx} inexistante")))?;
    if params.len() != arguments.len() {
        return err(format!(
            "'{export}' attend {} argument(s), {} reçu(s)",
            params.len(), arguments.len()
        ));
    }
    let args: Vec<ValWasm> = arguments.iter().zip(params.iter())
        .map(|(v, t)| match t {
            TyWasm::I32 => ValWasm::I32(*v as i32),
            TyWasm::I64 => ValWasm::I64(*v),
            TyWasm::F32 => ValWasm::F32(*v as f32),
            TyWasm::F64 => ValWasm::F64(*v as f64),
        })
        .collect();
    let mut inst = crate::wasm_lecteur::InstanceWasm::nouveau(module);
    let res = inst.appeler(export, &args)
        .map_err(|e| ErreurRuntime(format!("appel de '{export}' : {e}")))?;
    Ok(res.iter().map(|v| v.as_i64()).collect())
}

/// Exécute un export wasm.
///
/// * `runtime == None` → interpréteur maison (défaut).
/// * `runtime == Some(rt)` → runner officiel SI déjà installé ; SINON repli
///   explicite sur l'interpréteur maison, avec la raison dans
///   `Resultat::repli`. **Aucun téléchargement n'a lieu ici.**
pub fn executer_export(
    chemin_wasm: &Path,
    export: &str,
    arguments: &[i64],
    runtime: Option<RuntimeOfficiel>,
) -> Result<Resultat, ErreurRuntime> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(rt) = runtime {
        if let Some(runner) = trouver_runtime(rt) {
            let valeurs = lancer_runner(&runner, chemin_wasm, export, arguments)?;
            return Ok(Resultat { voie: Voie::Officiel, valeurs, repli: None });
        }
        let valeurs = executer_maison(chemin_wasm, export, arguments)?;
        return Ok(Resultat {
            voie: Voie::Maison,
            valeurs,
            repli: Some(format!(
                "runtime officiel '{}' absent du cache — repli sur l'interpréteur maison \
                 (installer avec `rose wasm --install-runtime {}`)",
                rt.nom(), rt.nom()
            )),
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = runtime; // pas de processus externe en WebAssembly : maison uniquement

    let valeurs = executer_maison(chemin_wasm, export, arguments)?;
    Ok(Resultat { voie: Voie::Maison, valeurs, repli: None })
}

/// Lance le runner officiel : `runner <module.wasm> <export> [args…]`.
#[cfg(not(target_arch = "wasm32"))]
fn lancer_runner(
    runner: &Path,
    chemin_wasm: &Path,
    export: &str,
    arguments: &[i64],
) -> Result<Vec<i64>, ErreurRuntime> {
    use std::process::Command;

    let mut cmd = Command::new(runner);
    cmd.arg(chemin_wasm).arg(export);
    for a in arguments { cmd.arg(a.to_string()); }
    let sortie = cmd.output().map_err(|e| {
        ErreurRuntime(format!("lancement du runner {} : {e}", runner.display()))
    })?;
    if !sortie.status.success() {
        return err(format!(
            "runner {} a échoué ({}) : {}",
            runner.display(),
            sortie.status,
            String::from_utf8_lossy(&sortie.stderr).trim()
        ));
    }
    parser_resultats(&String::from_utf8_lossy(&sortie.stdout))
}

/// Analyse la sortie du runner : un entier signé décimal par ligne.
/// Les lignes vides et les commentaires (`#`) sont ignorés.
pub fn parser_resultats(sortie: &str) -> Result<Vec<i64>, ErreurRuntime> {
    let mut out = Vec::new();
    for ligne in sortie.lines() {
        let l = ligne.trim();
        if l.is_empty() || l.starts_with('#') { continue; }
        let valeur = match l.parse::<i64>() {
            Ok(v) => v,
            Err(_) => {
                // Tolérance : certains runners préfixent la valeur (`result: 42`).
                match l.split_whitespace().last().and_then(|t| t.parse::<i64>().ok()) {
                    Some(v) => v,
                    None => return err(format!("sortie du runner non numérique : '{l}'")),
                }
            }
        };
        out.push(valeur);
    }
    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════════
// TESTS (fonctions pures : pas de réseau, pas de binaire externe)
// ═══════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_deterministes() {
        assert_eq!(nom_fichier(RuntimeOfficiel::Wasmtime).starts_with("wasmtime-"), true);
        assert_eq!(nom_fichier(RuntimeOfficiel::Wasmer).starts_with("wasmer-"), true);
        assert!(nom_fichier(RuntimeOfficiel::Wasmtime).contains(VERSION_RUNTIME));
        assert_ne!(nom_fichier(RuntimeOfficiel::Wasmtime), nom_fichier(RuntimeOfficiel::Wasmer));
    }

    #[test]
    fn depuis_str() {
        assert_eq!(RuntimeOfficiel::depuis_str("officiel"), Some(RuntimeOfficiel::Wasmtime));
        assert_eq!(RuntimeOfficiel::depuis_str("WASMTIME"), Some(RuntimeOfficiel::Wasmtime));
        assert_eq!(RuntimeOfficiel::depuis_str("wasmer"), Some(RuntimeOfficiel::Wasmer));
        assert_eq!(RuntimeOfficiel::depuis_str("inconnu"), None);
    }

    #[test]
    fn verification_sha256() {
        // `sha256_hex` est réutilisé (vecteur NIST « abc »).
        let abc = crate::sha256::sha256_hex(b"abc");
        assert!(verifier_sha256(b"abc", &abc).is_ok());
        assert!(verifier_sha256(b"abc", &abc.to_uppercase()).is_ok());
        assert!(verifier_sha256(b"abd", &abc).is_err());
        assert!(verifier_sha256(b"abc", "").is_err());
    }

    #[test]
    fn analyse_sortie_runner() {
        assert_eq!(parser_resultats("196418\n"), Ok(vec![196_418]));
        assert_eq!(parser_resultats("# note\n\n42\nresult: 7\n"), Ok(vec![42, 7]));
        assert_eq!(parser_resultats("-1\n0\n"), Ok(vec![-1, 0]));
        assert!(parser_resultats("pas un nombre\n").is_err());
    }
}
