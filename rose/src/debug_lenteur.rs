// ═══════════════════════════════════════════════════════════════════════
// `rose debug-lenteur` — OÙ VA LE TEMPS ?
// ═══════════════════════════════════════════════════════════════════════
//
// ⚠⚠⚠ POURQUOI CET OUTIL. Mesuré chez l'utilisateur : « quand je fais une
// commande rose, elle met un certain temps à afficher l'output », et
// « avant, ça ne le faisait pas ». Or le moteur démarre en 0,00 s sur
// Linux. Sur Windows, trois suspects seulement peuvent expliquer un délai
// AVANT la première sortie :
//
//   1. LE LANCEMENT DU PROCESSUS (`CreateProcess` + Defender qui scanne
//      un exécutable de 7,5 Mo non signé) — le délai est alors AVANT
//      l'entrée dans `main` ;
//   2. LA PRÉPARATION DE LA CONSOLE (`SetConsoleTitleW`, chargement de
//      l'icône) — juste après l'entrée dans `main` ;
//   3. LE PROGRAMME LUI-MÊME (analyse, vérification, JIT) — après.
//
// ⚠ On MESURE, on ne devine pas. Chaque étape est chronométrée et le total
// est ventilé. L'outil rejoue la commande PLUSIEURS fois (le premier
// lancement est toujours plus lent : cache disque froid, scan Defender) et
// rend la MÉDIANE — c'est elle qui dit si le coût est constant ou non.
//
// ⚠ Ce module n'a AUCUNE dépendance : il n'utilise que `std`, donc il
// compile sur toutes les cibles (Linux, Windows, WASM sans intérêt).

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Une mesure isolée.
#[derive(Clone)]
struct Mesure {
    /// Ce qui est mesuré.
    nom: String,
    /// Durée observée.
    duree: Duration,
    /// Détail éventuel (code de sortie, octets lus…).
    detail: String,
}

/// Médiane d'une série de durées (au moins une).
fn mediane(v: &mut Vec<Duration>) -> Duration {
    if v.is_empty() { return Duration::ZERO; }
    v.sort();
    v[v.len() / 2]
}

fn ms(d: Duration) -> String {
    let micro = d.as_micros();
    if micro < 1_000 {
        format!("{micro} µs")
    } else if micro < 1_000_000 {
        format!("{:.2} ms", micro as f64 / 1_000.0)
    } else {
        format!("{:.3} s", micro as f64 / 1_000_000.0)
    }
}

/// Lance un programme et rend (durée, code, octets de stdout).
///
/// ⚠ `Stdio::null()` pour stdin : un programme qui attend une frappe (cas
/// de la console éphémère sous Windows) ne doit pas bloquer la mesure.
fn chronometrer(programme: &str, args: &[&str]) -> (Duration, Option<i32>, usize) {
    let debut = Instant::now();
    let sortie = Command::new(programme)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let duree = debut.elapsed();
    match sortie {
        Ok(o) => (duree, o.status.code(), o.stdout.len()),
        Err(_) => (duree, None, 0),
    }
}

/// Lance l'outil et écrit le rapport.
///
/// `args` : `["debug-lenteur", "--repetitions", "N", "--programme", "f.rose"]`
/// (tous optionnels).
pub fn execute(args: &[String]) -> i32 {
    let mut repetitions: usize = 5;
    let mut programme: Option<String> = None;
    let mut i = 2; // args[0] = binaire, args[1] = « debug-lenteur »
    while i < args.len() {
        match args[i].as_str() {
            "--repetitions" | "-n" if i + 1 < args.len() => {
                repetitions = args[i + 1].parse().unwrap_or(5).max(1).min(50);
                i += 2;
            }
            "--programme" | "-p" if i + 1 < args.len() => {
                programme = Some(args[i + 1].clone());
                i += 2;
            }
            autre => { programme = Some(autre.to_string()); i += 1; }
        }
    }

    // ⚠ Le binaire à mesurer est NOUS-MÊMES : c'est le seul moyen de
    // mesurer exactement ce que l'utilisateur tape.
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("rose: binaire introuvable ({e})");
            return 1;
        }
    };
    let exe = exe.display().to_string();

    println!("Rose {} — diagnostic de lenteur", env!("CARGO_PKG_VERSION"));
    println!("Binaire : {exe}");
    println!("Répétitions : {repetitions} (on garde la MÉDIANE)\n");

    let mut mesures: Vec<Mesure> = Vec::new();

    // ── 1. Démarrage minimal : `version` ─────────────────────────────────
    //
    // ⚠ C'est LA mesure clé. `rose version` n'analyse rien, ne compile
    // rien : son temps est celui du LANCEMENT DU PROCESSUS (CreateProcess
    // + scan antivirus) PLUS la préparation de la console.
    let mut temps = Vec::new();
    let mut code = None;
    let mut octets = 0;
    for _ in 0..repetitions {
        let (d, c, o) = chronometrer(&exe, &["version"]);
        temps.push(d);
        code = c;
        octets = o;
    }
    let premiere = temps.first().copied().unwrap_or_default();
    let med = mediane(&mut temps);
    mesures.push(Mesure {
        nom: "lancement (rose version)".into(),
        duree: med,
        detail: format!("code {code:?}, {octets} o de sortie, 1er = {}", ms(premiere)),
    });

    // ── 2. `info` : lecture de l'état du moteur ──────────────────────────
    let mut temps = Vec::new();
    for _ in 0..repetitions {
        let (d, _, _) = chronometrer(&exe, &["info"]);
        temps.push(d);
    }
    mesures.push(Mesure {
        nom: "état (rose info)".into(),
        duree: mediane(&mut temps),
        detail: String::new(),
    });

    // ── 3. Un programme MINUSCULE (analyse + exécution) ──────────────────
    //
    // ⚠ On ne mesure pas un fichier externe : on écrit un programme
    // minimal dans le dossier temporaire pour que la mesure soit
    // reproductible sur n'importe quelle machine.
    let tmp = std::env::temp_dir().join("rose-debug-lenteur.rose");
    let _ = std::fs::write(&tmp, "afficher(\"bonjour\")\n");
    let tmp_str = tmp.display().to_string();
    let mut temps = Vec::new();
    for _ in 0..repetitions {
        let (d, _, _) = chronometrer(&exe, &[&tmp_str]);
        temps.push(d);
    }
    let med_petit = mediane(&mut temps);
    mesures.push(Mesure {
        nom: "programme minimal".into(),
        duree: med_petit,
        detail: format!("surcoût d'exécution = {}", ms(med_petit.saturating_sub(med))),
    });

    // ── 4. Le programme de l'utilisateur, s'il en donne un ───────────────
    if let Some(p) = &programme {
        if !std::path::Path::new(p).is_file() {
            println!("⚠ programme introuvable : {p}\n");
        } else {
            let mut temps = Vec::new();
            let mut code = None;
            for _ in 0..repetitions {
                let (d, c, _) = chronometrer(&exe, &[p]);
                temps.push(d);
                code = c;
            }
            let premiere = temps.first().copied().unwrap_or_default();
            let med_p = mediane(&mut temps);
            mesures.push(Mesure {
                nom: format!("programme {}", p),
                duree: med_p,
                detail: format!("code {code:?}, 1er = {}, surcoût = {}",
                                ms(premiere), ms(med_p.saturating_sub(med))),
            });
        }
    }

    // ── 5. L'analyse seule (sans exécution) : `latex` ────────────────────
    //
    // ⚠ `rose latex` ANALYSE le programme mais ne l'exécute pas : la
    // différence avec le programme minimal dit si le coût est dans
    // l'analyse (parseur) ou dans l'exécution (JIT).
    let mut temps = Vec::new();
    for _ in 0..repetitions {
        let (d, _, _) = chronometrer(&exe, &["latex", &tmp_str]);
        temps.push(d);
    }
    mesures.push(Mesure {
        nom: "analyse seule (rose latex)".into(),
        duree: mediane(&mut temps),
        detail: String::new(),
    });

    let _ = std::fs::remove_file(&tmp);

    // ── Rapport ──────────────────────────────────────────────────────────
    println!("{:<32} {:>12}   détail", "étape", "médiane");
    println!("{}", "─".repeat(78));
    for m in &mesures {
        println!("{:<32} {:>12}   {}", m.nom, ms(m.duree), m.detail);
    }
    println!();

    // ── Interprétation ───────────────────────────────────────────────────
    //
    // ⚠ On ne dit pas « c'est l'antivirus » : on compare les étapes pour
    // DÉSIGNER la coupable. Trois verdicts possibles.
    let lancement = mesures[0].duree;
    let analyse = mesures.last().map(|m| m.duree).unwrap_or_default();
    println!("INTERPRÉTATION");
    if lancement >= Duration::from_millis(200) {
        println!("  ⚠ Le LANCEMENT coûte {} : le délai est AVANT l'entrée dans le",
                 ms(lancement));
        println!("    moteur (CreateProcess, scan antivirus d'un .exe non signé,");
        println!("    dossier hors des exclusions Defender). `rose version` ne fait");
        println!("    RIEN et coûte autant → ce n'est pas le langage.");
        println!("    → ajoutez le dossier d'installation aux exclusions Defender,");
        println!("      ou testez sur un autre PC pour comparer.");
    } else {
        println!("  ✓ Le lancement coûte {} : pas de surcoût notable au démarrage.",
                 ms(lancement));
    }
    if analyse > lancement * 3 / 2 {
        println!("  ⚠ L'ANALYSE coûte {} : le temps est dans le parseur.",
                 ms(analyse));
    }
    if mesures[2].duree > analyse * 3 / 2 {
        println!("  ⚠ L'EXÉCUTION coûte {} : le temps est dans le JIT/interpréteur.",
                 ms(mesures[2].duree));
    }
    println!();

    // ── Détail Windows : ce que l'outil ne peut pas mesurer d'ici ────────
    #[cfg(windows)]
    {
        println!("COMPLÉMENTS WINDOWS");
        println!("  · préparation de la console : titre + icône (console_win)");
        println!("  · si `rose version` est lent ici mais rapide ailleurs → antivirus");
        println!("  · exclusions Defender : Sécurité Windows → Protection contre");
        println!("    les virus → Exclusions → dossier  {}",
                 std::path::Path::new(&exe).parent()
                     .map(|p| p.display().to_string())
                     .unwrap_or_default());
    }
    #[cfg(not(windows))]
    {
        println!("COMPLÉMENTS");
        println!("  · ces chiffres sont ceux de CETTE machine ({}).", std::env::consts::OS);
        println!("    Sur Windows, comparez `rose version` ici et dans un dossier");
        println!("    exclu de Defender : la différence est le coût de l'antivirus.");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mediane_prend_la_valeur_du_milieu() {
        let mut v = vec![Duration::from_millis(30), Duration::from_millis(10),
                         Duration::from_millis(20)];
        assert_eq!(mediane(&mut v), Duration::from_millis(20));
        let mut v2 = vec![Duration::from_millis(1), Duration::from_millis(9),
                          Duration::from_millis(5), Duration::from_millis(7)];
        // 4 éléments : l'indice 2 → 7 ms.
        assert_eq!(mediane(&mut v2), Duration::from_millis(7));
        assert_eq!(mediane(&mut vec![]), Duration::ZERO);
    }

    #[test]
    fn ms_formate_lisiblement() {
        assert_eq!(ms(Duration::from_micros(500)), "500 µs");
        assert_eq!(ms(Duration::from_millis(12)), "12.00 ms");
        assert_eq!(ms(Duration::from_millis(1500)), "1.500 s");
    }
}
