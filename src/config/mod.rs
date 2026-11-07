use std::env;
use std::path::{Path, PathBuf};

pub mod animation;
pub mod theme;
pub mod watchable;

pub use watchable::WatchableConfig;

/// `$XDG_CONFIG_HOME/my_shell/<name>`, or the checkout's `example/<name>` until
/// the user has their own copy.
pub fn config_path(name: &str) -> PathBuf {
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
