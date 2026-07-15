use std::path::Path;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use tokio::sync::mpsc;

/// Default file extensions to watch for changes.
const DEFAULT_WATCH_EXTENSIONS: &[&str] = &["py", "toml", "yaml", "yml", "json", "env"];

/// Directories that are always excluded from file watching.
const EXCLUDED_DIRS: &[&str] = &[
    "__pycache__",
    ".git",
    "target",
    "node_modules",
    ".venv",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "__pypackages__",
];

/// Returns `true` if the given path should be ignored because it resides
/// inside one of the [`EXCLUDED_DIRS`].
pub fn should_ignore_path(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_str().map(|s| EXCLUDED_DIRS.contains(&s)).unwrap_or(false))
}

/// Returns `true` if `path` has one of the given extensions.
fn has_watched_extension(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| extensions.iter().any(|w| w == ext))
        .unwrap_or(false)
}

/// Spawn a background file watcher that monitors `watch_dir` for changes.
///
/// When a relevant file change is detected the watcher sends `()` on the
/// returned [`mpsc::Receiver`].  The caller can `recv()` to learn about
/// each reload trigger.
///
/// # Arguments
///
/// * `watch_dir` — directory to watch recursively.
/// * `extra_extensions` — additional file extensions to watch beyond the
///   defaults.  Pass an empty slice to use only the built-in set.
pub fn spawn_file_watcher(
    watch_dir: &Path,
    extra_extensions: &[String],
) -> Result<mpsc::Receiver<()>, anyhow::Error> {
    // Build the full extension list.
    let mut extensions: Vec<String> =
        DEFAULT_WATCH_EXTENSIONS.iter().map(|s| (*s).to_owned()).collect();
    for ext in extra_extensions {
        let ext = ext.strip_prefix('.').unwrap_or(ext).to_owned();
        if !extensions.contains(&ext) {
            extensions.push(ext);
        }
    }

    // Channel between the notify callback (sync) and the async task.
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<String>(16);

    // Channel returned to the caller.
    let (reload_tx, reload_rx) = mpsc::channel::<()>(1);

    let exts = extensions.clone();
    let mut debouncer =
        new_debouncer(Duration::from_millis(300), move |result: DebounceEventResult| {
            if let Ok(events) = result {
                for event in &events {
                    if should_ignore_path(&event.path) {
                        continue;
                    }
                    if has_watched_extension(&event.path, &exts) {
                        let display = event
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("<unknown>")
                            .to_owned();
                        notify_tx.try_send(display).ok();
                        break;
                    }
                }
            }
        })?;

    debouncer.watcher().watch(watch_dir, RecursiveMode::Recursive)?;

    tokio::spawn(async move {
        // Keep the debouncer alive as long as the task runs.
        let _debouncer = debouncer;
        while let Some(changed_file) = notify_rx.recv().await {
            tracing::info!(file = %changed_file, "File change detected, triggering reload");
            if reload_tx.send(()).await.is_err() {
                // Receiver dropped — stop watching.
                break;
            }
        }
    });

    Ok(reload_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_should_ignore_excluded_directories() {
        // Paths inside excluded directories must be ignored.
        let cases = vec![
            ("project/__pycache__/module.pyc", true),
            ("project/.git/config", true),
            ("project/target/debug/binary", true),
            ("project/node_modules/pkg/index.js", true),
            ("project/.venv/lib/python3.14/site.py", true),
            (".mypy_cache/report.json", true),
            (".pytest_cache/v/cache/lastfailed", true),
            (".ruff_cache/0.1.0/foo.json", true),
            ("__pypackages__/3.14/lib/pkg.py", true),
        ];
        for (path, expected) in cases {
            assert_eq!(
                should_ignore_path(Path::new(path)),
                expected,
                "should_ignore_path({path:?}) should be {expected}"
            );
        }
    }

    #[test]
    fn test_should_not_ignore_normal_directories() {
        // Regular project files must NOT be ignored.
        let cases = vec![
            "app/main.py",
            "config.toml",
            "src/lib.rs",
            "app/routes/users.py",
            "tests/test_api.py",
            "static/style.css",
        ];
        for path in cases {
            assert!(
                !should_ignore_path(Path::new(path)),
                "should_ignore_path({path:?}) should be false"
            );
        }
    }

    #[test]
    fn test_has_watched_extension_matches() {
        let exts: Vec<String> = vec!["py", "toml", "json"].into_iter().map(String::from).collect();
        assert!(has_watched_extension(&PathBuf::from("app/main.py"), &exts));
        assert!(has_watched_extension(&PathBuf::from("config.toml"), &exts));
        assert!(has_watched_extension(&PathBuf::from("data.json"), &exts));
    }

    #[test]
    fn test_has_watched_extension_rejects_non_matching() {
        let exts: Vec<String> = vec!["py", "toml", "json"].into_iter().map(String::from).collect();
        assert!(!has_watched_extension(&PathBuf::from("style.css"), &exts));
        assert!(!has_watched_extension(&PathBuf::from("binary"), &exts));
        assert!(!has_watched_extension(&PathBuf::from("readme.md"), &exts));
    }
}
