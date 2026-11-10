//! What the providers report, as signals every output's bar reads. The providers keep their
//! threads and send here through `ui_channel`s, whose closures run on the UI thread.

use std::{
    collections::HashMap,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

use anti_webui::prelude::*;
use log::{debug, info, warn};
use mpris::{Event as MprisEvent, PlaybackStatus};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    cli::{self, Action},
    config::{animation::AnimationConfig, config_path, theme::Theme, WatchableConfig},
    dbus::{
        self,
        kdeconnect::{KDEConnectClient, KDEConnectCommand, KDEConnectEvent},
        mpris::{PlayerCommand, PlayerState},
        notification::{CloseReason, NotificationEvent, NotificationHandle, NotificationStore},
        player_art::Art,
        warp::{WarpClient, WarpCommand, WarpStatus},
    },
    ipc::{self, events::IPCEvent},
    system::{self, SystemCommand, SystemStatus},
};

/// How often notifications are checked for expiry.
const CLOCK_TICK: Duration = Duration::from_secs(1);

/// Where commands to the providers go. Dropping a sender closes its provider's command
/// channel, so these live as long as the bar even where nothing issues commands yet.
#[allow(unused)]
pub struct Commands {
    pub system: UnboundedSender<SystemCommand>,
    pub warp: UnboundedSender<WarpCommand>,
    pub player: mpsc::Sender<PlayerCommand>,
    pub kdeconnect: UnboundedSender<KDEConnectCommand>,
    /// `None` when another notification daemon holds the name.
    pub notifications: Option<NotificationHandle>,
}

#[derive(Clone)]
#[allow(unused)] // ponytail: read by the components as Phase 2 ports them.
pub struct State {
    /// Keyed by bus name: two instances of a player share an identity.
    pub players: Signal<HashMap<String, PlayerState>>,
    /// Keyed by bus name, like `players`.
    pub player_art: Signal<HashMap<String, Art>>,
    /// The bus name of the player that played last, which the media island shows.
    pub last_playing: Signal<Option<String>>,
    pub warp: Signal<Option<WarpStatus>>,
    pub notifications: Signal<NotificationStore>,
    pub system: Signal<SystemStatus>,
    pub focused_title: Signal<Option<String>>,
    pub focused_workspace: Signal<Option<u64>>,
    pub theme: Signal<Theme>,
    pub animation: Signal<AnimationConfig>,
    /// The last action from `my_shell action`, with a count so the same one twice still
    /// reruns what reads it.
    pub action: Signal<Option<(u64, Action)>>,
    pub commands: Rc<Commands>,
}

impl State {
    /// Starts every provider and wires it to its signal. Call on the UI thread, inside
    /// `Shell::run`. Fails if another bar already listens on the CLI socket.
    pub fn start() -> std::io::Result<Self> {
        let players = signal(HashMap::<String, PlayerState>::new());
        let player_art = signal(HashMap::new());
        let last_playing = signal(None);
        let warp = signal(None);
        let notifications = signal(NotificationStore::default());
        let system = signal(SystemStatus::default());
        let focused_title = signal(None);
        let focused_workspace = signal(None);
        let action = signal(None::<(u64, Action)>);

        let action_tx = ui_channel(move |a: Action| {
            info!("CLI action: {a:?}");
            action.update(|last| *last = Some((last.as_ref().map_or(0, |(n, _)| n + 1), a)));
        });
        cli::listen(move |a| action_tx.send(a))?;

        let theme = signal(Theme::default());
        let theme_tx = ui_channel(move |t: Theme| theme.set(t));
        theme.set(Theme::load_and_watch(&config_path("theme.toml"), move |t| {
            theme_tx.send(t)
        }));
        let animation = signal(AnimationConfig::default());
        let animation_tx = ui_channel(move |a: AnimationConfig| animation.set(a));
        animation.set(AnimationConfig::load_and_watch(
            &config_path("animation.toml"),
            move |a| animation_tx.send(a),
        ));

        // Losing the name to another notification daemon costs only this feature.
        let notification_handle = NotificationHandle::init(ui_channel(move |e: NotificationEvent| {
            debug!("{e:?}");
            notifications.update(|n| n.apply(&e, Instant::now()));
        }))
        .map_err(|e| warn!("Notification server unavailable: {e}"))
        .ok();

        let player = dbus::mpris::start_listener(
            ui_channel(move |(state, event): (PlayerState, MprisEvent)| {
                debug!("{event:?}");
                if state.status == PlaybackStatus::Playing {
                    last_playing.set(Some(state.bus_name.clone()));
                }
                players.update(|p| {
                    if state.active {
                        p.insert(state.bus_name.clone(), state);
                    } else {
                        p.remove(&state.bus_name);
                    }
                });
            }),
            // Polled positions arrive often and mostly agree: only a drift wakes the readers.
            ui_channel(move |(bus_name, position, at): (String, Duration, Instant)| {
                let drifted = players.update_untracked(|p| {
                    p.get_mut(&bus_name)
                        .is_some_and(|player| player.sync_position(position, at))
                });
                if drifted {
                    players.notify();
                }
            }),
        );

        dbus::player_art::start_watcher(ui_channel(
            move |(bus_name, art): (String, Option<Art>)| {
                player_art.update(|a| match art {
                    Some(art) => a.insert(bus_name, art),
                    None => a.remove(&bus_name),
                });
            },
        ));

        ipc::listen(ui_channel(move |event: IPCEvent| match event {
            IPCEvent::FocusedWorkspaceChanged(_, id) => focused_workspace.set(Some(id)),
            IPCEvent::FocusedWindowTitleChanged(title) => focused_title.set(title),
        }));

        // ponytail: no component reads these yet; Phase 2 decides what they feed.
        let kdeconnect = KDEConnectClient::init(ui_channel(|e: KDEConnectEvent| debug!("{e:?}")));

        let warp_commands = WarpClient::init(ui_channel(move |s: WarpStatus| warp.set(Some(s))));
        let system_commands = system::start(ui_channel(move |s: SystemStatus| system.set(s)));

        let state = Self {
            players,
            player_art,
            last_playing,
            warp,
            notifications,
            system,
            focused_title,
            focused_workspace,
            theme,
            animation,
            action,
            commands: Rc::new(Commands {
                system: system_commands,
                warp: warp_commands,
                player,
                kdeconnect,
                notifications: notification_handle,
            }),
        };

        let ticking = state.clone();
        let tick = ui_channel(move |()| ticking.expire_notifications());
        std::thread::spawn(move || loop {
            std::thread::sleep(CLOCK_TICK);
            tick.send(());
        });

        Ok(state)
    }

    /// Dismisses the notification on display or among the missed ones.
    #[allow(unused)]
    pub fn dismiss_notification(&self, id: u32) {
        if self.notifications.update(|n| n.remove(id)) {
            self.notify_closed(id, CloseReason::Dismissed);
        }
    }

    /// Dismisses every notification, missed ones included.
    #[allow(unused)]
    pub fn clear_notifications(&self) {
        for id in self.notifications.update(|n| n.clear()) {
            self.notify_closed(id, CloseReason::Dismissed);
        }
    }

    fn expire_notifications(&self) {
        // ponytail: expiry is only as precise as the tick. Give it its own timer if a
        // notification ever needs sub-second timing.
        let expired = self
            .notifications
            .update_untracked(|n| n.take_expired(Instant::now()));
        if expired.is_empty() {
            return;
        }
        self.notifications.notify();
        for id in expired {
            self.notify_closed(id, CloseReason::Expired);
        }
    }

    /// Tells the notification's sender it is gone.
    fn notify_closed(&self, id: u32, reason: CloseReason) {
        if let Some(handle) = &self.commands.notifications {
            if let Err(e) = handle.close(id, reason) {
                warn!("Failed to signal notification {id} closed: {e}");
            }
        }
    }
}
