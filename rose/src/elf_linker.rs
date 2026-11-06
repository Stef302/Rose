// Rose 2.0 — Linker ELF (délégation à cc/ld pour la résolution libc).
// Le backend LLVM ORC produit des objets .o ne référençant que printf
// (libc). On délègue le lien à `cc`/`gcc`/`ld` du système, ce qui résout
// automatiquement libc et produit un ELF exécutable autonome.
// Les symboles printf.N générés par LLVM sont renommés en printf.

use std::process::Command;

fn normalize_printf(obj_path: &str) -> Result<String, String> {
    // Copier l'objet dans un fichier temporaire et renommer printf.N -> printf
    let tmp = format!("{}.norm.o", obj_path);
    std::fs::copy(obj_path, &tmp)
        .map_err(|e| format!("copie objet : {}", e))?;

    // Lister les symboles undefined
    let out = Command::new("nm")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("nm : {}", e))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0] == "U" {
            let sym = parts[1];
            if sym.starts_with("printf.") {
                let _ = Command::new("objcopy")
                    .args(["--redefine-sym", &format!("{}=printf", sym), &tmp])
                    .status();
            }
        }
    }
    Ok(tmp)
}

pub fn link_elf(objet_path: &str, sortie_path: &str) -> Result<(), String> {
    let norm_path = normalize_printf(objet_path).unwrap_or_else(|_| objet_path.to_string());

    let status = Command::new("cc")
        .args([&norm_path, "-o", sortie_path])
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => {
            let status2 = Command::new("ld")
                .args([&norm_path, "-o", sortie_path, "-lc", "-dynamic-linker", "/lib/ld-linux-aarch64.so.1"])
                .status();
            match status2 {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => Err(format!("ld a échoué (code {})", s.code().unwrap_or(-1))),
                Err(e) => Err(format!("impossible d'invoquer ld : {}", e)),
            }
        }
    }
}

pub fn link_elf_multi(objets_paths: &[&str], sortie_path: &str) -> Result<(), String> {
    if objets_paths.is_empty() {
        return Err("aucun objet à lier".to_string());
    }
    let mut cmd = Command::new("cc");
    for p in objets_paths {
        let norm = normalize_printf(p).unwrap_or_else(|_| p.to_string());
        cmd.arg(norm);
    }
    cmd.arg("-o").arg(sortie_path);
    let status = cmd.status()
        .map_err(|e| format!("impossible d'invoquer cc : {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cc a échoué (code {})", status.code().unwrap_or(-1)))
    }
}
