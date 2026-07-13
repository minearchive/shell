use std::thread;
use std::time::Duration;

use calloop::channel::Sender;
use futures_util::StreamExt;
use serde::Deserialize;
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
    #[allow(unused)]
    pub fn is_connected(&self) -> bool {
        self.status.eq_ignore_ascii_case("connected")
    }
}

#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WarpCommand {
    Connect,
    Disconnect,
    UpdateState,
}

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
        emit(
            &sender,
            &mut last,
            query_status().await,
            WarpCommand::UpdateState,
        );

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    run_command(cmd.clone()).await;
                    emit(&sender, &mut last, query_status().await, cmd);
                }
                Some(_) = sni.next() => {
                    emit(&sender, &mut last, query_status().await, WarpCommand::UpdateState);
                }
                _ = poll.tick() => {
                    emit(&sender, &mut last, query_status().await, WarpCommand::UpdateState);
                }
            }
        }
    }
}

fn emit(
    sender: &Sender<WarpStatus>,
    last: &mut Option<WarpStatus>,
    status: Option<WarpStatus>,
    cmd: WarpCommand,
) {
    let Some(status) = status else { return };
    if last.as_ref() == Some(&status) && cmd != WarpCommand::UpdateState {
        return;
    }
    let _ = sender.send(status.clone());
    *last = Some(status);
}

async fn query_status() -> Option<WarpStatus> {
    let output = tokio::task::spawn_blocking(|| {
        std::process::Command::new("warp-cli")
            .args(["-j", "--accept-tos", "status"])
            .output()
    })
    .await
    .ok()?
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

async fn run_command(cmd: WarpCommand) {
    let arg = match cmd {
        WarpCommand::Connect => "connect",
        WarpCommand::Disconnect => "disconnect",
        WarpCommand::UpdateState => {
            return; //no call command, emit are update state
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("warp-cli")
            .args(["--accept-tos", arg])
            .output()
    })
    .await;

    match result {
        Ok(Ok(out)) if !out.status.success() => log::warn!(
            "warp: `warp-cli {arg}` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Ok(Err(e)) => log::warn!("warp: could not run `warp-cli {arg}`: {e}"),
        Err(e) => log::warn!("warp: `warp-cli {arg}` task failed: {e}"),
        _ => {}
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

    #[tokio::test]
    #[ignore]
    async fn reads_live_status() {
        let status = query_status().await.expect("warp-cli status");
        println!("live warp status: {status:?}");
    }
}
