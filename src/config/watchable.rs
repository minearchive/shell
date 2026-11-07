use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;

use calloop::channel::Sender;
use log::{info, warn};
use notify::{RecursiveMode, Watcher};
use serde::de::DeserializeOwned;

use crate::ui::UiEvent;

pub trait WatchableConfig: Default + Sized + Send + Sync + 'static {
    fn try_load(path: &Path) -> Option<Self>;
    fn is_changed(&self, new: &Self) -> bool;
    fn reload_event(&self) -> UiEvent;

    fn load(path: &Path) -> Self {
        Self::try_load(path).unwrap_or_default()
    }

    fn load_and_watch(path: &Path, ui_tx: Sender<UiEvent>) -> Arc<RwLock<Self>> {
        let config = Arc::new(RwLock::new(Self::load(path)));
        let config_ref = Arc::clone(&config);
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
                        if config_ref.read().unwrap().is_changed(&new_cfg) {
                            let event = new_cfg.reload_event();
                            *config_ref.write().unwrap() = new_cfg;
                            info!("Config reloaded.");
                            let _ = ui_tx.send(event);
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
    use std::time::{Duration, Instant};

    use serde::Deserialize;

    use super::*;

    #[derive(Default, Deserialize, PartialEq)]
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
        fn reload_event(&self) -> UiEvent {
            UiEvent::RequestRedrawAll
        }
    }

    #[test]
    fn reloads_after_atomic_rename_saves() {
        let dir = std::env::temp_dir().join(format!("my_shell_watch_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cfg.toml");
        fs::write(&path, "value = 1").unwrap();

        let (tx, _rx) = calloop::channel::channel();
        let config = Cfg::load_and_watch(&path, tx);
        assert_eq!(config.read().unwrap().value, 1);

        // Two saves: the second proves the watch survived the first's rename.
        for value in [2, 3] {
            thread::sleep(Duration::from_millis(100));
            let tmp = dir.join("cfg.toml.tmp");
            fs::write(&tmp, format!("value = {value}")).unwrap();
            fs::rename(&tmp, &path).unwrap();

            let start = Instant::now();
            while config.read().unwrap().value != value {
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "no reload to {value}"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }

        fs::remove_dir_all(&dir).unwrap();
    }
}
