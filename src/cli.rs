use std::{
    env, fs,
    io::{self, BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    thread,
    time::Duration,
};

use clap::{Parser, Subcommand};
use log::warn;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    /// Directory holding theme.toml, animation.toml, …
    #[arg(short = 'c', long)]
    pub config_directory: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Send an action to the running bar
    Action {
        #[command(subcommand)]
        action: Action,
    },
}

/// What `my_shell action …` asks the running bar to do. It travels over the
/// socket as one line of JSON and reaches components through `on_action`.
#[derive(Subcommand, Serialize, Deserialize, Debug, Clone)]
pub enum Action {
    /// Show the launcher
    Launcher,
}

fn socket_path() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("my_shell.sock")
}

/// Sends `action` to the running bar and waits for it to acknowledge.
pub fn send(action: &Action) -> Result<(), String> {
    let path = socket_path();
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("my_shell is not running ({}: {e})", path.display()))?;
    let line = serde_json::to_string(action).map_err(|e| e.to_string())?;
    writeln!(stream, "{line}").map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    match reply.trim_end() {
        "ok" => Ok(()),
        error => Err(error.to_owned()),
    }
}

/// Binds the socket and hands every action sent to it to `on_action`, on the
/// socket's thread. Fails if another instance is already listening.
pub fn listen(on_action: impl Fn(Action) + Send + 'static) -> io::Result<()> {
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("another my_shell is listening on {}", path.display()),
        ));
    }
    // Left behind by an instance that did not exit cleanly.
    let _ = fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(e) = serve(stream, &on_action) {
                        warn!("CLI connection failed: {e}");
                    }
                }
                Err(e) => warn!("CLI socket accept failed: {e}"),
            }
        }
    });
    Ok(())
}

fn serve(stream: UnixStream, on_action: &dyn Fn(Action)) -> io::Result<()> {
    // A client that never finishes its line must not block the next one.
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let reply = match serde_json::from_str::<Action>(&line) {
        Ok(action) => {
            on_action(action);
            "ok".to_owned()
        }
        // Only a CLI from a different build sends something this one can't read.
        Err(e) => format!("unknown action {:?}: {e}", line.trim_end()),
    };
    writeln!(&stream, "{reply}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(line: &str) -> (String, Option<Action>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (client, server) = UnixStream::pair().unwrap();
        writeln!(&client, "{line}").unwrap();
        serve(server, &|action| tx.send(action).unwrap()).unwrap();
        let mut reply = String::new();
        BufReader::new(&client).read_line(&mut reply).unwrap();
        (reply, rx.try_recv().ok())
    }

    #[test]
    fn serve_forwards_known_actions_and_rejects_others() {
        let line = serde_json::to_string(&Action::Launcher).unwrap();
        let (reply, action) = roundtrip(&line);
        assert_eq!(reply, "ok\n");
        assert!(matches!(action, Some(Action::Launcher)));

        let (reply, action) = roundtrip("\"Nope\"");
        assert!(reply.starts_with("unknown action"), "{reply}");
        assert!(action.is_none());
    }
}
