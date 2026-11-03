//! Runtime location of the extracted game data, shared by every crate (`sekiro_sim::cache`).
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static CACHE: OnceLock<PathBuf> = OnceLock::new();

/// Set the process-wide location before constructing simulation or game resources.
pub fn init(explicit: Option<&Path>) {
    CACHE.get_or_init(|| resolve(explicit));
}

/// The CLI override, environment override, nearest executable ancestor, or working directory.
pub fn resolve(explicit: Option<&Path>) -> PathBuf {
    let environment = std::env::var_os("SEKIRO_CACHE").map(PathBuf::from);
    let executable = std::env::current_exe().ok();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_with(
        explicit,
        environment.as_deref(),
        executable.as_deref(),
        &cwd,
        Path::is_dir,
    )
}

/// Use the initialized CLI location, or discover the data for non-CLI callers.
pub fn root() -> PathBuf {
    CACHE.get_or_init(|| resolve(None)).clone()
}

fn resolve_with(
    explicit: Option<&Path>,
    environment: Option<&Path>,
    executable: Option<&Path>,
    cwd: &Path,
    is_dir: impl Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(path) = explicit.or(environment) {
        return cwd.join(path);
    }
    if let Some(dir) = executable.and_then(Path::parent) {
        for ancestor in dir.ancestors() {
            let candidate = ancestor.join("cache");
            if is_dir(&candidate) {
                return candidate;
            }
        }
    }
    cwd.join("cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_cache_precedence() {
        let cwd = std::env::temp_dir().join("sekiro-cache-test");
        let base = cwd.join("installation");
        let executable = base.join("bin/game.exe");
        let next_to_exe = base.join("bin/cache");
        let parent = base.join("cache");
        for (explicit, environment, existing, expected) in [
            (
                Some("chosen"),
                Some("environment"),
                vec![next_to_exe.clone()],
                cwd.join("chosen"),
            ),
            (
                None,
                Some("environment"),
                vec![next_to_exe.clone()],
                cwd.join("environment"),
            ),
            (
                None,
                None,
                vec![parent.clone(), next_to_exe.clone()],
                next_to_exe,
            ),
            (None, None, vec![parent.clone()], parent),
            (None, None, vec![], cwd.join("cache")),
        ] {
            assert_eq!(
                resolve_with(
                    explicit.map(Path::new),
                    environment.map(Path::new),
                    Some(&executable),
                    &cwd,
                    |path| existing.iter().any(|p| p == path)
                ),
                expected
            );
        }
        assert_eq!(resolve_with(Some(&base), None, None, &cwd, |_| false), base);
    }
}
