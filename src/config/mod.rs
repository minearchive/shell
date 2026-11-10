use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub mod animation;
pub mod theme;
pub mod watchable;

pub use watchable::WatchableConfig;

static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Overrides where `config_path` looks. Call before loading any config.
pub fn set_config_dir(dir: PathBuf) {
    CONFIG_DIR.set(dir).expect("config directory set twice");
}

/// `<--config-directory>/<name>` if given, else `$XDG_CONFIG_HOME/my_shell/<name>`,
/// or the checkout's `example/<name>` until the user has their own copy.
pub fn config_path(name: &str) -> PathBuf {
    if let Some(dir) = CONFIG_DIR.get() {
        return dir.join(name);
    }
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
        .map(|dir| dir.join("my_shell").join(name))
        .filter(|path| path.exists())
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("example")
                .join(name)
        })
}
