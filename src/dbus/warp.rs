use std::thread;
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::process::Command;
use tokio::sync::mpsc;
use zbus::{Connection, MatchRule, MessageStream};

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct WarpStatus {
    pub status: String,
    #[serde(default, deserialize_with = "deserialize_reason")]
    pub reason: Option<String>,
}

fn deserialize_reason<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;
    Ok(match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s),
        Some(Value::Object(map)) => map.into_iter().next().map(|(key, _)| key),
        Some(other) => Some(other.to_string()),
    })
}

impl WarpStatus {
    pub fn is_connected(&self) -> bool {
        self.status.eq_ignore_ascii_case("connected")
    }

    pub fn is_connecting(&self) -> bool {
        self.status.eq_ignore_ascii_case("connecting")
    }

    fn connecting() -> Self {
        Self {
            status: "Connecting".to_string(),
            reason: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WarpCommand {
    Connect,
    Disconnect,
}

/// How soon to look again after a command, while the connection is still changing.
const SETTLING_POLL: Duration = Duration::from_secs(1);
/// The daemon may still read as disconnected this long after accepting a connect.
const CONNECT_GRACE: Duration = Duration::from_secs(3);

pub struct WarpClient;

impl WarpClient {
    pub fn init(sender: Sender<WarpStatus>) -> mpsc::UnboundedSender<WarpCommand> {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<WarpCommand>();

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("warp: build runtime");

            rt.block_on(async move {
                if let Err(e) = Self::run(sender, cmd_rx).await {
                    log::error!("warp: {e}");
                }
            });
        });

        cmd_tx
    }

    async fn run(
        sender: Sender<WarpStatus>,
        mut cmd_rx: mpsc::UnboundedReceiver<WarpCommand>,
    ) -> zbus::Result<()> {
        let connection = Connection::session().await?;

        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.kde.StatusNotifierItem")?
            .build();
        let mut sni = MessageStream::for_match_rule(rule, &connection, Some(16)).await?;

        let mut poll = tokio::time::interval(Duration::from_secs(5));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut last: Option<WarpStatus> = None;
        let mut grace_until: Option<Instant> = None;

        // The first poll tick fires immediately, delivering the initial status.
        loop {
            let mut settling = false;
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    // Shown right away: the command itself can take a moment.
                    let connecting = cmd == WarpCommand::Connect;
                    if connecting {
                        let _ = sender.send(WarpStatus::connecting());
                        last = Some(WarpStatus::connecting());
                    }
                    let accepted = run_command(cmd).await;
                    grace_until = (connecting && accepted).then(|| Instant::now() + CONNECT_GRACE);
                    settling = true;
                }
                Some(_) = sni.next() => {}
                _ = poll.tick() => {}
            }

            if let Some(status) = query_status().await {
                let status = hide_stale_disconnect(status, grace_until, Instant::now());
                settling |= status.is_connecting();
                if last.as_ref() != Some(&status) {
                    let _ = sender.send(status.clone());
                    last = Some(status);
                }
            }
            if settling {
                poll.reset_after(SETTLING_POLL);
            }
        }
    }
}

/// Keeps a connect that was just accepted from flickering back to disconnected.
fn hide_stale_disconnect(
    status: WarpStatus,
    grace_until: Option<Instant>,
    now: Instant,
) -> WarpStatus {
    let stale = status.status.eq_ignore_ascii_case("disconnected")
        && grace_until.is_some_and(|until| now < until);
    if stale {
        WarpStatus::connecting()
    } else {
        status
    }
}

async fn query_status() -> Option<WarpStatus> {
    let output = Command::new("warp-cli")
        .args(["-j", "--accept-tos", "status"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        log::warn!(
            "warp: `warp-cli status` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }

    match serde_json::from_slice::<WarpStatus>(&output.stdout) {
        Ok(status) => Some(status),
        Err(e) => {
            log::warn!("warp: could not parse status json: {e}");
            None
        }
    }
}

/// Returns whether the daemon accepted the command.
async fn run_command(cmd: WarpCommand) -> bool {
    let arg = match cmd {
        WarpCommand::Connect => "connect",
        WarpCommand::Disconnect => "disconnect",
    };

    match Command::new("warp-cli")
        .args(["--accept-tos", arg])
        .output()
        .await
    {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            log::warn!(
                "warp: `warp-cli {arg}` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            false
        }
        Err(e) => {
            log::warn!("warp: could not run `warp-cli {arg}`: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_json() {
        let json = br#"{ "status": "Connected", "reason": "NetworkHealthy" }"#;
        let status: WarpStatus = serde_json::from_slice(json).unwrap();
        assert_eq!(status.status, "Connected");
        assert_eq!(status.reason.as_deref(), Some("NetworkHealthy"));
        assert!(status.is_connected());
    }

    #[test]
    fn parses_status_without_reason() {
        let status: WarpStatus =
            serde_json::from_slice(br#"{ "status": "Disconnected" }"#).unwrap();
        assert_eq!(status.reason, None);
        assert!(!status.is_connected());
    }

    #[test]
    fn collapses_tagged_reason_object() {
        let json =
            br#"{ "status": "Disconnected", "reason": { "SettingsChanged": { "current": {} } } }"#;
        let status: WarpStatus = serde_json::from_slice(json).unwrap();
        assert_eq!(status.reason.as_deref(), Some("SettingsChanged"));
        assert!(!status.is_connected());
    }

    #[test]
    fn disconnected_reads_as_connecting_only_during_the_grace() {
        let now = Instant::now();
        let read = |status: &str, until| {
            let status = WarpStatus {
                status: status.to_string(),
                reason: None,
            };
            hide_stale_disconnect(status, until, now).status
        };
        let later = Some(now + CONNECT_GRACE);

        assert_eq!(read("Disconnected", later), "Connecting");
        assert_eq!(read("Connected", later), "Connected");
        assert_eq!(read("Disconnected", Some(now)), "Disconnected");
        assert_eq!(read("Disconnected", None), "Disconnected");
    }

    #[tokio::test]
    #[ignore]
    async fn reads_live_status() {
        let status = query_status().await.expect("warp-cli status");
        println!("live warp status: {status:?}");
    }
}
