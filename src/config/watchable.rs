use std::io;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use calloop::channel::Sender;
use log::info;
use notify::{Config, RecursiveMode, Watcher};

use crate::ui::UiEvent;

#[allow(unused)]
pub trait WatchableConfig: Default + Sized + Send + Sync + 'static {
    fn try_load(path: &str) -> Option<Self>;
    fn write(&self, path: &str) -> io::Result<()>;
    fn is_changed(&self, new: &Self) -> bool;
    fn reload_event(&self) -> UiEvent;

    fn load(path: &str) -> Self {
        Self::try_load(path).unwrap_or_default()
    }

    fn load_and_watch(path: &str, ui_tx: Sender<UiEvent>) -> Arc<RwLock<Self>> {
        let config = Arc::new(RwLock::new(Self::load(path)));
        let config_ref = Arc::clone(&config);
        let path = path.to_string();

        thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut watcher = notify::recommended_watcher(tx).unwrap();
            let _ =
                watcher.configure(Config::default().with_poll_interval(Duration::from_millis(100)));

            watcher
                .watch(Path::new(&path), RecursiveMode::NonRecursive)
                .unwrap();

            for event in rx.into_iter().flatten() {
                if event.kind.is_modify() {
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
