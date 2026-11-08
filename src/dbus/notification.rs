use std::collections::HashMap;
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use skia_safe::Image;
use zbus::connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::Value;
use zbus::{interface, Connection};

use super::notification_icon;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// How long a notification stays up when its sender leaves the choice to us.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
/// How many expired notifications are kept for the toolbox; the oldest go first.
const MISSED_LIMIT: usize = 50;

/// Urgency level carried in the `urgency` hint (a byte).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
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
    /// Already icon-sized, from whichever of the sender's sources worked.
    pub icon: Option<Image>,
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

pub struct ActiveNotification {
    pub id: u32,
    pub data: NotificationSnapshot,
    pub posted: Instant,
    expires: Option<Instant>,
}

/// The notifications on display, and the ones that expired before anyone dismissed
/// them, both oldest first.
#[derive(Default)]
pub struct NotificationStore {
    active: Vec<ActiveNotification>,
    missed: Vec<ActiveNotification>,
}

impl NotificationStore {
    pub fn apply(&mut self, event: &NotificationEvent, now: Instant) {
        match event {
            NotificationEvent::Posted { id, data } => {
                // Per spec, critical notifications never expire on their own.
                let expires = match (data.urgency, data.expire_timeout) {
                    (Urgency::Critical, _) | (_, 0) => None,
                    (_, timeout) if timeout < 0 => Some(now + DEFAULT_TIMEOUT),
                    (_, timeout) => Some(now + Duration::from_millis(timeout as u64)),
                };
                let entry = ActiveNotification {
                    id: *id,
                    data: data.clone(),
                    posted: now,
                    expires,
                };
                self.missed.retain(|n| n.id != *id);
                match self.active.iter_mut().find(|n| n.id == *id) {
                    Some(existing) => *existing = entry,
                    None => self.active.push(entry),
                }
            }
            NotificationEvent::Closed { id } => {
                self.remove(*id);
            }
        }
    }

    /// The one to show: most urgent first, then newest.
    pub fn current(&self) -> Option<&ActiveNotification> {
        self.active.iter().max_by_key(|n| n.data.urgency)
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// Removes `id` wherever it is. Returns whether it was on display, so that its
    /// sender still has to be told.
    pub fn remove(&mut self, id: u32) -> bool {
        self.missed.retain(|n| n.id != id);
        let before = self.active.len();
        self.active.retain(|n| n.id != id);
        self.active.len() != before
    }

    /// Removes everything. Returns the ids that were on display, like [`Self::remove`].
    pub fn clear(&mut self) -> Vec<u32> {
        self.missed.clear();
        self.active.drain(..).map(|n| n.id).collect()
    }

    /// How many [`Self::unseen`] returns, without sorting them.
    pub fn unseen_len(&self) -> usize {
        self.active.len() + self.missed.len()
    }

    /// Every notification nobody has dismissed yet, newest first.
    pub fn unseen(&self) -> Vec<&ActiveNotification> {
        let mut all: Vec<_> = self.missed.iter().chain(&self.active).rev().collect();
        all.sort_by_key(|n| std::cmp::Reverse(n.posted));
        all
    }

    /// Moves the notifications whose time is up to the missed ones, and returns their ids.
    pub fn take_expired(&mut self, now: Instant) -> Vec<u32> {
        let (expired, active) = self
            .active
            .drain(..)
            .partition(|n| n.expires.is_some_and(|at| at <= now));
        self.active = active;
        let ids = expired.iter().map(|n| n.id).collect();
        self.missed.extend(expired);
        let excess = self.missed.len().saturating_sub(MISSED_LIMIT);
        self.missed.drain(..excess);
        ids
    }
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

        let icon = notification_icon::resolve(&app_icon, &hints);

        let data = NotificationSnapshot {
            app_name,
            icon,
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
        vec!["body".to_string(), "icon-static".to_string()]
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

pub struct NotificationHandle {
    conn: Connection,
}

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
    #[allow(unused)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn posted(id: u32, urgency: Urgency, expire_timeout: i32) -> NotificationEvent {
        NotificationEvent::Posted {
            id,
            data: NotificationSnapshot {
                urgency,
                expire_timeout,
                ..Default::default()
            },
        }
    }

    #[test]
    fn shows_most_urgent_then_newest() {
        let now = Instant::now();
        let mut store = NotificationStore::default();
        assert!(store.current().is_none());

        store.apply(&posted(1, Urgency::Normal, -1), now);
        store.apply(&posted(2, Urgency::Normal, -1), now);
        assert_eq!(store.current().unwrap().id, 2);

        store.apply(&posted(3, Urgency::Critical, -1), now);
        store.apply(&posted(4, Urgency::Normal, -1), now);
        assert_eq!(store.current().unwrap().id, 3);
    }

    #[test]
    fn expiry_follows_timeout_and_urgency() {
        let now = Instant::now();
        let mut store = NotificationStore::default();
        store.apply(&posted(1, Urgency::Normal, -1), now);
        store.apply(&posted(2, Urgency::Normal, 0), now);
        store.apply(&posted(3, Urgency::Normal, 100), now);
        store.apply(&posted(4, Urgency::Critical, 100), now);

        assert_eq!(
            store.take_expired(now + Duration::from_millis(99)),
            [0u32; 0]
        );
        assert_eq!(store.take_expired(now + Duration::from_millis(100)), [3]);
        assert_eq!(store.take_expired(now + DEFAULT_TIMEOUT), [1]);
        assert_eq!(
            store.take_expired(now + Duration::from_secs(3600)),
            [0u32; 0]
        );
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn replacing_keeps_one_entry_and_restarts_expiry() {
        let now = Instant::now();
        let later = now + DEFAULT_TIMEOUT;
        let mut store = NotificationStore::default();
        store.apply(&posted(1, Urgency::Normal, -1), now);
        store.apply(&posted(1, Urgency::Normal, -1), later);

        assert_eq!(store.len(), 1);
        assert!(store.take_expired(later).is_empty());
    }

    #[test]
    fn closing_removes_it() {
        let now = Instant::now();
        let mut store = NotificationStore::default();
        store.apply(&posted(1, Urgency::Normal, -1), now);
        store.apply(&NotificationEvent::Closed { id: 1 }, now);

        assert_eq!(store.len(), 0);
        assert!(!store.remove(1));
    }

    #[test]
    fn expired_ones_stay_unseen_until_dismissed_or_closed() {
        let now = Instant::now();
        let ids = |store: &NotificationStore| -> Vec<u32> {
            store.unseen().iter().map(|n| n.id).collect()
        };
        let mut store = NotificationStore::default();
        store.apply(&posted(1, Urgency::Normal, 100), now);
        store.apply(&posted(2, Urgency::Normal, 100), now);
        store.apply(&posted(3, Urgency::Normal, 0), now);
        assert_eq!(store.take_expired(now + DEFAULT_TIMEOUT), [1, 2]);

        assert_eq!(store.len(), 1, "only 3 is still on display");
        assert_eq!(ids(&store), [3, 2, 1]);

        assert!(!store.remove(2), "its sender already heard it expire");
        store.apply(&NotificationEvent::Closed { id: 1 }, now);
        assert_eq!(ids(&store), [3]);

        store.apply(&posted(4, Urgency::Normal, 100), now);
        store.take_expired(now + DEFAULT_TIMEOUT);
        assert_eq!(store.clear(), [3]);
        assert!(store.unseen().is_empty());
    }

    #[test]
    fn missed_ones_are_capped() {
        let now = Instant::now();
        let mut store = NotificationStore::default();
        for id in 0..MISSED_LIMIT as u32 + 5 {
            store.apply(&posted(id, Urgency::Normal, 100), now);
        }
        store.take_expired(now + DEFAULT_TIMEOUT);
        let unseen = store.unseen();
        assert_eq!(unseen.len(), MISSED_LIMIT);
        assert_eq!(unseen.last().unwrap().id, 5, "the oldest went first");
    }
}
