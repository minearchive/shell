use std::fs;
use std::path::Path;
use std::thread;

use log::{info, warn};
use notify::{RecursiveMode, Watcher};
use serde::de::DeserializeOwned;

pub trait WatchableConfig: Clone + Default + Sized + Send + 'static {
    fn try_load(path: &Path) -> Option<Self>;
    fn is_changed(&self, new: &Self) -> bool;

    fn load(path: &Path) -> Self {
        Self::try_load(path).unwrap_or_default()
    }

    /// Loads `path`, and hands every changed version of it to `on_reload` from a
    /// watcher thread.
    fn load_and_watch(path: &Path, on_reload: impl Fn(Self) + Send + 'static) -> Self {
        let config = Self::load(path);
        let mut current = config.clone();
        let path = path.to_path_buf();

        thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = notify::recommended_watcher(tx).unwrap();

            // Watch the directory: an editor that saves by rename replaces the
            // file's inode, which would silently end a watch on the file itself.
            let dir = path.parent().unwrap_or(Path::new("."));
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                warn!("Cannot watch {}: {e}", dir.display());
                return;
            }

            for event in rx.into_iter().flatten() {
                let touches_config = (event.kind.is_modify() || event.kind.is_create())
                    && event
                        .paths
                        .iter()
                        .any(|p| p.file_name() == path.file_name());
                if touches_config {
                    if let Some(new_cfg) = Self::try_load(&path) {
                        if current.is_changed(&new_cfg) {
                            current = new_cfg.clone();
                            info!("Config reloaded.");
                            on_reload(new_cfg);
                        }
                    }
                }
            }
        });

        config
    }
}

pub(super) fn read_toml<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let src = fs::read_to_string(path)
        .map_err(|e| warn!("{}: read failed: {e}", path.display()))
        .ok()?;

    if src.trim().is_empty() {
        return None;
    }

    toml::from_str(&src)
        .map_err(|e| warn!("{}: parse failed:\n{e}", path.display()))
        .ok()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde::Deserialize;

    use super::*;

    #[derive(Clone, Default, Deserialize, PartialEq)]
    struct Cfg {
        value: u32,
    }

    impl WatchableConfig for Cfg {
        fn try_load(path: &Path) -> Option<Self> {
            read_toml(path)
        }
        fn is_changed(&self, new: &Self) -> bool {
            self != new
        }
    }

    #[test]
    fn reloads_after_atomic_rename_saves() {
        let dir = std::env::temp_dir().join(format!("my_shell_watch_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cfg.toml");
        fs::write(&path, "value = 1").unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let config = Cfg::load_and_watch(&path, move |c| tx.send(c.value).unwrap());
        assert_eq!(config.value, 1);

        // Two saves: the second proves the watch survived the first's rename.
        for value in [2, 3] {
            thread::sleep(Duration::from_millis(100));
            let tmp = dir.join("cfg.toml.tmp");
            fs::write(&tmp, format!("value = {value}")).unwrap();
            fs::rename(&tmp, &path).unwrap();

            loop {
                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(got) if got == value => break,
                    Ok(_) => continue,
                    Err(_) => panic!("no reload to {value}"),
                }
            }
        }

        fs::remove_dir_all(&dir).unwrap();
    }
}
