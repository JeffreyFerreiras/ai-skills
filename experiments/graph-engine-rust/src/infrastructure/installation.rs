use std::path::{Path, PathBuf};

/// Installed packages may be renamed by a skill manager. Locate their resources
/// next to the executable rather than retaining the build machine's checkout.
pub fn root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|executable| installed_root(&executable))
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../skills/software-engineering-graph")
        })
}

fn installed_root(executable: &Path) -> Option<PathBuf> {
    let scripts = executable.parent().filter(|path| path.ends_with("scripts"))?;
    let skill = scripts.parent()?;
    (skill.join("SKILL.md").is_file() && skill.join("references").is_dir())
        .then(|| skill.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_renamed_installed_skill() {
        let temporary = tempfile::tempdir().unwrap();
        let skill = temporary.path().join("skill-managed-id");
        std::fs::create_dir_all(skill.join("scripts")).unwrap();
        std::fs::create_dir(skill.join("references")).unwrap();
        std::fs::write(skill.join("SKILL.md"), "test").unwrap();
        assert_eq!(installed_root(&skill.join("scripts/graphctl")), Some(skill));
    }

    #[test]
    fn ignores_unpackaged_build_binary() {
        assert_eq!(installed_root(Path::new("target/debug/graphctl")), None);
    }
}
