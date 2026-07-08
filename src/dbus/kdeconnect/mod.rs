use std::{collections::HashMap, fmt::Display, thread};

use calloop::channel::Sender;

mod proxy;
use futures_util::StreamExt;
pub use proxy::*;
use tokio::sync::mpsc;
use zbus::proxy::CacheProperties;
use zbus::Connection;

#[allow(unused)]
#[derive(Clone, Debug)]
pub enum KDEConnectEvent {
    DeviceConnected {
        name: String,
        id: String,
    },
    DeviceDisconnected {
        name: String,
        id: String,
        reason: String,
    },
    FileTransferRequest(String),        //idk params
    FileTransferRequestReject(String),  //idk to
    SMSMessage(String, String, String), //number, name, detail
    PhoneCall {
        number: String,
        name: String,
        call_type: PhoneCallType,
    },
    NotificationPosted {
        device_id: String,
        id: String,
        data: NotificationSnapshot,
    },
    NotificationUpdated {
        device_id: String,
        id: String,
        data: NotificationSnapshot,
    },
    /// The object is already gone by the time this fires, so it carries only
    /// the key needed to drop the matching widget entry.
    NotificationRemoved {
        device_id: String,
        id: String,
    },
    /// The device cleared every notification at once (`allNotificationsRemoved`).
    AllNotificationsRemoved {
        device_id: String,
    },
}

impl Display for KDEConnectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceConnected { name, id } => {
                write!(f, "device connected: {name} ({id})")
            }
            Self::DeviceDisconnected { name, id, reason } => {
                write!(f, "device disconnected: {name} ({id}): {reason}")
            }
            Self::FileTransferRequest(info) => write!(f, "file transfer request: {info}"),
            Self::FileTransferRequestReject(to) => {
                write!(f, "file transfer rejected: {to}")
            }
            Self::SMSMessage(number, name, detail) => {
                write!(f, "sms from {name} <{number}>: {detail}")
            }
            Self::PhoneCall {
                number,
                name,
                call_type,
            } => write!(f, "{call_type} call: {name} <{number}>"),
            Self::NotificationPosted { id, data, .. } => {
                write!(f, "notification [{id}]: {} - {}", data.title, data.text)
            }
            Self::NotificationUpdated { id, data, .. } => {
                write!(
                    f,
                    "notification updated [{id}]: {} - {}",
                    data.title, data.text
                )
            }
            Self::NotificationRemoved { device_id, id } => {
                write!(f, "notification removed: {device_id}/{id}")
            }
            Self::AllNotificationsRemoved { device_id } => {
                write!(f, "all notifications removed: {device_id}")
            }
        }
    }
}

pub struct DeviceHandles {
    name: String,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for DeviceHandles {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[derive(Clone, Debug)]
pub enum PhoneCallType {
    Ringing,
    Talking,
    MissedCall,
    Unknown,
}

impl From<String> for PhoneCallType {
    fn from(value: String) -> Self {
        match value.as_str() {
            "ringing" => Self::Ringing,
            "talking" => Self::Talking,
            "missedCall" => Self::MissedCall,
            _ => Self::Unknown,
        }
    }
}

impl Display for PhoneCallType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Ringing => "ringing",
            Self::Talking => "talking",
            Self::MissedCall => "missed",
            Self::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

#[allow(unused)]
#[derive(Clone, Debug, Default)]
pub struct NotificationSnapshot {
    pub app_name: String,
    pub title: String,
    pub text: String,
    pub ticker: String,
    pub icon_path: String,
    pub group_name: String,
    pub reply_id: String,
    pub dismissable: bool,
    pub silent: bool,
    pub is_conversation: bool,
}

impl NotificationSnapshot {
    pub async fn from_proxy(proxy: &NotificationProxy<'_>) -> Self {
        Self {
            app_name: proxy.app_name().await.unwrap_or_default(),
            title: proxy.title().await.unwrap_or_default(),
            text: proxy.text().await.unwrap_or_default(),
            ticker: proxy.ticker().await.unwrap_or_default(),
            icon_path: proxy.icon_path().await.unwrap_or_default(),
            group_name: proxy.group_name().await.unwrap_or_default(),
            reply_id: proxy.reply_id().await.unwrap_or_default(),
            dismissable: proxy.dismissable().await.unwrap_or_default(),
            silent: proxy.silent().await.unwrap_or_default(),
            is_conversation: proxy.is_conversation().await.unwrap_or_default(),
        }
    }
}

/// Build a cache-free proxy for `<device_path>/notifications/<public_id>` and
/// snapshot it. Returns `None` if the object can't be built (e.g. it was already
/// removed between the signal and this read).
async fn read_notification(
    connection: &Connection,
    device_path: &str,
    public_id: &str,
) -> Option<NotificationSnapshot> {
    let path = format!("{device_path}/notifications/{public_id}");
    let proxy = NotificationProxy::builder(connection)
        .path(path)
        .ok()?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;
    Some(NotificationSnapshot::from_proxy(&proxy).await)
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub enum KDEConnectCommand {
    SendPing { device_id: String },
    Ring { device_id: String },
    ShareText { device_id: String, text: String },
}

pub struct KDEConnectClient {}

impl KDEConnectClient {
    pub fn init(sender: Sender<KDEConnectEvent>) -> mpsc::UnboundedSender<KDEConnectCommand> {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<KDEConnectCommand>();
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("RT");

            rt.block_on(async move {
                if let Err(e) = Self::run(sender, cmd_rx).await {
                    log::error!("kdeconnect: {e}");
                }
            });
        });
        cmd_tx
    }

    async fn run(
        sender: Sender<KDEConnectEvent>,
        mut cmd_rx: mpsc::UnboundedReceiver<KDEConnectCommand>,
    ) -> zbus::Result<()> {
        let connection = zbus::Connection::session().await?;
        let daemon = DaemonProxy::new(&connection).await?;

        let mut devices: HashMap<String, DeviceHandles> = HashMap::new();

        for id in daemon.devices_reachable(true).await? {
            Self::add_device(&connection, &id, &mut devices, sender.clone()).await?;
        }

        let mut device_added = daemon.receive_device_added().await?;
        let mut device_removed = daemon.receive_device_removed().await?;

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    if let Err(e) = Self::handle_command(&connection, cmd).await {
                        log::warn!("kdeconnect: command failed: {e}");
                    }
                },
                Some(sig) = device_added.next() => {
                    let id = sig.args()?.id().to_string();
                    if let Err(e) = Self::add_device(&connection, &id, &mut devices, sender.clone()).await {
                        log::warn!("kdeconnect: add_device {id}: {e}");
                    }
                },
                Some(sig) = device_removed.next() => {
                    let id = sig.args()?.id().to_string();
                    // Removing drops the DeviceHandles, aborting its listener tasks.
                    if let Some(handle) = devices.remove(&id) {
                        let _ = sender.send(KDEConnectEvent::DeviceDisconnected {
                            name: handle.name.clone(),
                            id: id,
                            reason: "Device Removed".to_string(),
                        });
                    }
                }
            }
        }
    }

    async fn handle_command(connection: &Connection, cmd: KDEConnectCommand) -> zbus::Result<()> {
        match cmd {
            KDEConnectCommand::SendPing { device_id } => {
                let ping = PingProxy::builder(connection)
                    .path(format!("/modules/kdeconnect/devices/{device_id}/ping"))?
                    .build()
                    .await?;
                ping.send_ping().await?;
            }
            KDEConnectCommand::Ring { device_id } => {
                let fmp = FindMyPhoneProxy::builder(connection)
                    .path(format!(
                        "/modules/kdeconnect/devices/{device_id}/findmyphone"
                    ))?
                    .build()
                    .await?;
                fmp.ring().await?;
            }
            KDEConnectCommand::ShareText { device_id, text } => {
                let share = ShareProxy::builder(connection)
                    .path(format!("/modules/kdeconnect/devices/{device_id}/share"))?
                    .build()
                    .await?;
                share.share_text(&text).await?;
            }
        }
        Ok(())
    }

    async fn add_device(
        connection: &Connection,
        id: &str,
        devices: &mut HashMap<String, DeviceHandles>,
        sender: Sender<KDEConnectEvent>,
    ) -> zbus::Result<()> {
        if devices.contains_key(id) {
            return Ok(());
        }

        let path = format!("/modules/kdeconnect/devices/{id}");
        let device = DeviceProxy::builder(connection)
            .path(path.clone())?
            .build()
            .await?;

        let name = device.name().await?;
        let _ = sender.send(KDEConnectEvent::DeviceConnected {
            name: name.clone(),
            id: id.to_string(),
        });

        let mut tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();

        // Reachability -> Connected / Disconnected
        {
            let (id, name, sender) = (id.to_string(), name.clone(), sender.clone());
            tasks.push(tokio::spawn(async move {
                let Ok(mut stream) = device.receive_reachable_changed().await else {
                    return;
                };
                while let Some(sig) = stream.next().await {
                    let Ok(args) = sig.args() else { continue };
                    let ev = if *args.reachable() {
                        KDEConnectEvent::DeviceConnected {
                            name: name.clone(),
                            id: id.clone(),
                        }
                    } else {
                        KDEConnectEvent::DeviceDisconnected {
                            name: name.clone(),
                            id: id.clone(),
                            reason: "Device unreachable".to_string(),
                        }
                    };
                    let _ = sender.send(ev);
                }
            }));
        }

        // Telephony -> PhoneCall
        {
            let telephony = TelephonyProxy::builder(connection)
                .path(format!("{path}/telephony"))?
                .build()
                .await?;
            let sender = sender.clone();
            tasks.push(tokio::spawn(async move {
                let Ok(mut stream) = telephony.receive_call_received().await else {
                    return;
                };
                while let Some(sig) = stream.next().await {
                    let Ok(args) = sig.args() else { continue };
                    let _ = sender.send(KDEConnectEvent::PhoneCall {
                        number: args.phone_number().to_string(),
                        name: args.contact_name().to_string(),
                        call_type: PhoneCallType::from(args.event),
                    });
                }
            }));
        }

        // Notifications -> NotificationPosted, NotificationUpdated, NotificationRemoved,
        // AllNotificationsRemoved. Every event carries (device_id, public_id) so a widget
        // can key notifications and drop them on removal without matching on text.
        {
            let notifications = NotificationsProxy::builder(connection)
                .path(format!("{path}/notifications"))?
                .build()
                .await?;
            let device_id = id.to_string();

            // notificationPosted -> read the object once into a snapshot.
            {
                let (notifications, connection, sender, path, device_id) = (
                    notifications.clone(),
                    connection.clone(),
                    sender.clone(),
                    path.clone(),
                    device_id.clone(),
                );
                tasks.push(tokio::spawn(async move {
                    let Ok(mut stream) = notifications.receive_notification_posted().await else {
                        return;
                    };
                    while let Some(sig) = stream.next().await {
                        let Ok(args) = sig.args() else { continue };
                        let id = args.public_id().to_string();
                        let Some(data) = read_notification(&connection, &path, &id).await else {
                            continue;
                        };
                        let _ = sender.send(KDEConnectEvent::NotificationPosted {
                            device_id: device_id.clone(),
                            id,
                            data,
                        });
                    }
                }));
            }

            // notificationUpdated -> same read, different event so the widget can
            // distinguish a fresh notification from a content change.
            {
                let (notifications, connection, sender, path, device_id) = (
                    notifications.clone(),
                    connection.clone(),
                    sender.clone(),
                    path.clone(),
                    device_id.clone(),
                );
                tasks.push(tokio::spawn(async move {
                    let Ok(mut stream) = notifications.receive_notification_updated().await else {
                        return;
                    };
                    while let Some(sig) = stream.next().await {
                        let Ok(args) = sig.args() else { continue };
                        let id = args.public_id().to_string();
                        let Some(data) = read_notification(&connection, &path, &id).await else {
                            continue;
                        };
                        let _ = sender.send(KDEConnectEvent::NotificationUpdated {
                            device_id: device_id.clone(),
                            id,
                            data,
                        });
                    }
                }));
            }

            // notificationRemoved -> the object is gone; forward only the key.
            {
                let (notifications, sender, device_id) =
                    (notifications.clone(), sender.clone(), device_id.clone());
                tasks.push(tokio::spawn(async move {
                    let Ok(mut stream) = notifications.receive_notification_removed().await else {
                        return;
                    };
                    while let Some(sig) = stream.next().await {
                        let Ok(args) = sig.args() else { continue };
                        let _ = sender.send(KDEConnectEvent::NotificationRemoved {
                            device_id: device_id.clone(),
                            id: args.public_id().to_string(),
                        });
                    }
                }));
            }

            // allNotificationsRemoved -> clear everything for this device at once.
            {
                let (notifications, sender, device_id) = (notifications, sender.clone(), device_id);
                tasks.push(tokio::spawn(async move {
                    let Ok(mut stream) = notifications.receive_all_notifications_removed().await
                    else {
                        return;
                    };
                    while stream.next().await.is_some() {
                        let _ = sender.send(KDEConnectEvent::AllNotificationsRemoved {
                            device_id: device_id.clone(),
                        });
                    }
                }));
            }
        }

        devices.insert(id.to_string(), DeviceHandles { name, tasks });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_devices() {
        let conn = zbus::Connection::session().await.unwrap();
        let proxy = DaemonProxy::new(&conn).await.unwrap();
        let devices = proxy.device_names().await.unwrap();
        println!("devices ({}):", devices.len());
        for id in &devices {
            println!("{:?}", id)
        }
    }
}
