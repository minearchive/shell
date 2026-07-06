use std::collections::HashMap;

use calloop::channel::Sender;
use zbus::connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{interface, proxy, Connection};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

// ==========================================================================
// Notifications — org.freedesktop.Notifications
// service: org.freedesktop.Notifications
// path:    /org/freedesktop/Notifications
// The Desktop Notifications Specification (v1.2). Method/signal names map to
// their PascalCase D-Bus names via zbus's default conversion, so no explicit
// `name =` is needed. `hints` is `a{sv}`; `actions` is a flat `as` list of
// (key, label, key, label, …) pairs.
// ==========================================================================
#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
pub trait Notifications {
    /// Post a notification. `replaces_id` of 0 requests a fresh id; any other
    /// value updates the notification with that id in place. Returns the id the
    /// server assigned (or echoed back).
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<&str>,
        hints: HashMap<&str, OwnedValue>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    /// Ask the server to close the notification with the given id.
    fn close_notification(&self, id: u32) -> zbus::Result<()>;

    /// The capabilities the server supports (e.g. "body", "actions",
    /// "body-markup", "persistence").
    fn get_capabilities(&self) -> zbus::Result<Vec<String>>;

    /// (name, vendor, version, spec_version) describing the running server.
    fn get_server_information(&self) -> zbus::Result<(String, String, String, String)>;

    /// A notification was closed. `reason`: 1 = expired, 2 = dismissed by the
    /// user, 3 = closed via `CloseNotification`, 4 = undefined/reserved.
    #[zbus(signal)]
    fn notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()>;

    /// The user invoked an action; `action_key` matches a key passed in the
    /// `actions` list of `Notify`.
    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;

    /// Carries the XDG activation token for the notification whose action was
    /// just invoked, letting the handler raise its window without focus steal.
    #[zbus(signal)]
    fn activation_token(&self, id: u32, activation_token: String) -> zbus::Result<()>;
}

/// Urgency level carried in the `urgency` hint (a byte).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl From<u8> for Urgency {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }
}

/// Why a notification was closed, per the spec's `NotificationClosed` reason.
#[allow(unused)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    Expired = 1,
    Dismissed = 2,
    Closed = 3,
    Undefined = 4,
}

/// A snapshot of one `Notify` call — everything the UI needs to render it.
#[allow(unused)]
#[derive(Clone, Debug, Default)]
pub struct NotificationSnapshot {
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    /// (key, label) pairs the user can invoke; the flat `as` list is paired up.
    pub actions: Vec<(String, String)>,
    pub urgency: Urgency,
    /// Milliseconds. `-1` = server default, `0` = never expire.
    pub expire_timeout: i32,
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub enum NotificationEvent {
    Posted { id: u32, data: NotificationSnapshot },
    Closed { id: u32 },
}

pub struct NotificationServer {
    sender: Sender<NotificationEvent>,
    next_id: u32,
}

impl NotificationServer {
    fn new(sender: Sender<NotificationEvent>) -> Self {
        Self { sender, next_id: 0 }
    }

    fn assign_id(&mut self, replaces_id: u32) -> u32 {
        if replaces_id != 0 {
            return replaces_id;
        }
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }
        self.next_id
    }
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let id = self.assign_id(replaces_id);

        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .map_or(Urgency::Normal, Urgency::from);

        let actions = actions
            .chunks_exact(2)
            .map(|pair| (pair[0].clone(), pair[1].clone()))
            .collect();

        let data = NotificationSnapshot {
            app_name,
            app_icon,
            summary,
            body,
            actions,
            urgency,
            expire_timeout,
        };

        let _ = self.sender.send(NotificationEvent::Posted { id, data });
        id
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let _ = self.sender.send(NotificationEvent::Closed { id });
        Self::notification_closed(&emitter, id, CloseReason::Closed as u32).await?;
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<String> {
        ["actions", "body", "body-markup"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "gtk_shell".to_string(),
            "minearchive".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: String,
    ) -> zbus::Result<()>;
}

#[allow(unused)]
pub struct NotificationHandle {
    conn: Connection,
}

#[allow(unused)]
impl NotificationHandle {
    pub fn init(sender: Sender<NotificationEvent>) -> zbus::Result<Self> {
        let conn = zbus::block_on(async move {
            connection::Builder::session()?
                .name(NAME)?
                .serve_at(PATH, NotificationServer::new(sender))?
                .build()
                .await
        })?;
        Ok(Self { conn })
    }

    pub fn close(&self, id: u32, reason: CloseReason) -> zbus::Result<()> {
        zbus::block_on(async {
            let iface = self
                .conn
                .object_server()
                .interface::<_, NotificationServer>(PATH)
                .await?;
            NotificationServer::notification_closed(iface.signal_emitter(), id, reason as u32).await
        })
    }

    /// Tell the sender the user clicked one of the notification's actions.
    pub fn invoke_action(&self, id: u32, action_key: &str) -> zbus::Result<()> {
        zbus::block_on(async {
            let iface = self
                .conn
                .object_server()
                .interface::<_, NotificationServer>(PATH)
                .await?;
            NotificationServer::action_invoked(iface.signal_emitter(), id, action_key.to_string())
                .await
        })
    }
}
