use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use log::{debug, error, info};
use mpris::{DBusError, Event, LoopStatus, PlaybackStatus, Player, PlayerFinder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAction {
    Previous,
    PlayPause,
    Next,
    SeekTo(Duration),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerCommand {
    pub bus_name: String,
    pub action: PlayerAction,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub bus_name: String,
    pub identity: String,
    pub active: bool,
    pub status: PlaybackStatus,
    pub title: Option<String>,
    pub artists: Option<Vec<String>>,
    /// Not every player reports one: without it there is nothing to seek through.
    pub length: Option<Duration>,
    /// Where playback was at `position_at`. Players don't announce it as it advances.
    pub position: Duration,
    pub position_at: Instant,
    pub volume: Option<f64>,
    pub shuffle: bool,
    pub loop_status: LoopStatus,
}

impl PlayerState {
    /// Snapshot a player we just found, which may be mid-song already.
    pub fn read(player: &Player) -> Self {
        Self {
            bus_name: player.bus_name().to_string(),
            identity: player.identity().to_string(),
            active: true,
            status: player
                .get_playback_status()
                .unwrap_or(PlaybackStatus::Stopped),
            title: None,
            artists: None,
            length: None,
            position: player.get_position().unwrap_or_default(),
            position_at: Instant::now(),
            volume: player.get_volume().ok(),
            shuffle: player.get_shuffle().unwrap_or(false),
            loop_status: player.get_loop_status().unwrap_or(LoopStatus::None),
        }
    }

    /// `(position, length)` as of `now`, for a player that reports a length.
    pub fn progress(&self, now: Instant) -> Option<(Duration, Duration)> {
        let length = self.length.filter(|length| !length.is_zero())?;
        let elapsed = if self.status == PlaybackStatus::Playing {
            now.saturating_duration_since(self.position_at)
        } else {
            Duration::ZERO
        };
        Some(((self.position + elapsed).min(length), length))
    }

    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::PlayerShutDown => {
                self.active = false;
                self.status = PlaybackStatus::Stopped;
            }
            Event::Playing => self.status = PlaybackStatus::Playing,
            Event::Paused => self.status = PlaybackStatus::Paused,
            Event::Stopped => self.status = PlaybackStatus::Stopped,
            Event::VolumeChanged(v) => self.volume = Some(*v),
            Event::ShuffleToggled(s) => self.shuffle = *s,
            Event::LoopingChanged(l) => self.loop_status = *l,
            Event::TrackChanged(meta) => {
                self.title = meta.title().map(str::to_owned);
                self.artists = meta
                    .artists()
                    .map(|v| v.into_iter().map(str::to_owned).collect());
                self.length = meta.length();
            }
            _ => {}
        }
    }
}

fn find_player(finder: &PlayerFinder, bus_name: &str) -> Option<Player> {
    finder
        .iter_players()
        .ok()?
        .filter_map(Result::ok)
        .find(|player| player.bus_name() == bus_name)
}

fn seek(player: &Player, position: Duration) -> Result<(), DBusError> {
    match player.get_metadata()?.track_id() {
        Some(track) => player.set_position(track, &position),
        None => Ok(()),
    }
}

/// Commands block on the player's reply, so they get a thread of their own.
fn start_commands() -> mpsc::Sender<PlayerCommand> {
    let (tx, rx) = mpsc::channel::<PlayerCommand>();
    thread::spawn(move || {
        let finder = match PlayerFinder::new() {
            Ok(finder) => finder,
            Err(err) => {
                error!("Failed to connect to D-Bus: {err}");
                return;
            }
        };
        for command in rx {
            let Some(player) = find_player(&finder, &command.bus_name) else {
                debug!("[{}] Player no longer present", command.bus_name);
                continue;
            };
            let result = match command.action {
                PlayerAction::Previous => player.previous(),
                PlayerAction::PlayPause => player.play_pause(),
                PlayerAction::Next => player.next(),
                PlayerAction::SeekTo(position) => seek(&player, position),
            };
            if let Err(err) = result {
                error!("[{}] {:?} failed: {err}", command.bus_name, command.action);
            }
        }
    });
    tx
}

pub fn start_listener(sender: Sender<(PlayerState, Event)>) -> mpsc::Sender<PlayerCommand> {
    thread::spawn(move || {
        let finder = match PlayerFinder::new() {
            Ok(f) => f,
            Err(err) => {
                error!("Failed to connect to D-Bus: {err}");
                return;
            }
        };

        let mut tracked: HashSet<String> = HashSet::new();
        let mut suppressed_errors: HashSet<String> = HashSet::new();

        loop {
            let mut current_errors: HashSet<String> = HashSet::new();

            let players: Vec<_> = match finder.iter_players() {
                Ok(iter) => iter
                    .filter_map(|res| match res {
                        Ok(player) => Some(player),
                        Err(err) => {
                            let msg = err.to_string();
                            if !suppressed_errors.contains(&msg) {
                                debug!("Skipping unreadable player: {err}");
                            }
                            current_errors.insert(msg);
                            None
                        }
                    })
                    .collect(),
                Err(err) => {
                    error!("Failed to list players: {err:?}");
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };

            let current: HashSet<String> =
                players.iter().map(|p| p.bus_name().to_string()).collect();

            tracked.retain(|bus_name| current.contains(bus_name));

            for player in &players {
                let bus_name = player.bus_name().to_string();
                if tracked.insert(bus_name.clone()) {
                    let sender = sender.clone();
                    info!("[{}] new player spawned. listening...", player.identity());
                    thread::spawn(move || {
                        read_events_for_player(bus_name, sender);
                    });
                }
            }

            suppressed_errors = current_errors;
            thread::sleep(Duration::from_millis(500));
        }
    });
    start_commands()
}

fn read_events_for_player(bus_name: String, sender: Sender<(PlayerState, Event)>) {
    let finder = match PlayerFinder::new() {
        Ok(finder) => finder,
        Err(err) => {
            error!("[{bus_name}] Could not connect to D-Bus: {err}");
            return;
        }
    };

    let Some(player) = find_player(&finder, &bus_name) else {
        error!("[{bus_name}] Player no longer present");
        return;
    };

    let events = match player.events() {
        Ok(events) => events,
        Err(err) => {
            error!(
                "[{}] Could not start event stream: {err}",
                player.identity()
            );
            return;
        }
    };

    let mut state = PlayerState::read(&player);

    if let Ok(metadata) = player.get_metadata() {
        let event = Event::TrackChanged(metadata);
        state.apply(&event);
        let _ = sender.send((state.clone(), event));
    }

    for event in events {
        match event {
            Ok(event) => {
                state.apply(&event);
                // Any change can move the position, and only a seek says where to.
                state.position = player.get_position().unwrap_or(state.position);
                state.position_at = Instant::now();
                let _ = sender.send((state.clone(), event));

                if !state.active {
                    info!("[{}] Player has been closed", state.identity);
                    break;
                }
            }
            Err(err) => {
                error!("[{}] D-Bus error: {err}. Aborting.", state.identity);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    fn state(status: PlaybackStatus, length: Option<u64>) -> PlayerState {
        PlayerState {
            bus_name: String::new(),
            identity: String::new(),
            active: true,
            status,
            title: None,
            artists: None,
            length: length.map(Duration::from_secs),
            position: Duration::from_secs(10),
            position_at: Instant::now(),
            volume: None,
            shuffle: false,
            loop_status: LoopStatus::None,
        }
    }

    #[test]
    fn progress_advances_only_while_playing_and_stops_at_the_end() {
        let in_5s = |state: &PlayerState| state.position_at + Duration::from_secs(5);
        let secs = Duration::from_secs;

        let playing = state(PlaybackStatus::Playing, Some(100));
        assert_eq!(
            playing.progress(in_5s(&playing)),
            Some((secs(15), secs(100)))
        );

        let paused = state(PlaybackStatus::Paused, Some(100));
        assert_eq!(paused.progress(in_5s(&paused)), Some((secs(10), secs(100))));

        let ending = state(PlaybackStatus::Playing, Some(12));
        assert_eq!(ending.progress(in_5s(&ending)), Some((secs(12), secs(12))));

        let unknown = state(PlaybackStatus::Playing, None);
        assert_eq!(unknown.progress(in_5s(&unknown)), None);
        let empty = state(PlaybackStatus::Playing, Some(0));
        assert_eq!(empty.progress(in_5s(&empty)), None);
    }

    #[test]
    #[ignore = "needs a live MPRIS player and runs until it quits"]
    fn test_run() {
        let finder = PlayerFinder::new().expect("Could not connect to D-Bus");
        let players = finder.find_all().expect("Could not list players");

        if players.is_empty() {
            println!("No MPRIS players found.");
            return;
        }

        println!(
            "Showing event stream for {} player(s)...\n(Exit with Ctrl-C)\n",
            players.len()
        );

        let start = Instant::now();

        let mut handles = Vec::new();
        for player in &players {
            let identity = player.identity().to_string();

            handles.push(thread::spawn(move || {
                let finder = match PlayerFinder::new() {
                    Ok(finder) => finder,
                    Err(err) => {
                        println!("[{identity}] Could not connect to D-Bus: {err}");
                        return;
                    }
                };

                let player = match finder.find_by_name(&identity) {
                    Ok(player) => player,
                    Err(err) => {
                        println!("[{identity}] Could not reopen player: {err}");
                        return;
                    }
                };

                let events = match player.events() {
                    Ok(events) => events,
                    Err(err) => {
                        println!("[{identity}] Could not start event stream: {err}");
                        return;
                    }
                };

                for event in events {
                    match event {
                        Ok(event) => println!(
                            "{} [{identity}]: {:#?}",
                            format_elapsed(start.elapsed()),
                            event
                        ),
                        Err(err) => {
                            println!("[{identity}] D-Bus error: {err}. Aborting.");
                            break;
                        }
                    }
                }

                println!("[{identity}] Event stream ended.");
            }));
        }

        for handle in handles {
            let _ = handle.join();
        }

        println!("All event streams ended.");
    }

    fn format_elapsed(duration: Duration) -> String {
        let seconds = duration.as_secs();
        let minutes = seconds / 60;
        let seconds_left = seconds - (60 * minutes);
        let ms = duration.subsec_millis();
        format!("{:02}:{:02}.{:3}", minutes, seconds_left, ms)
    }
}
