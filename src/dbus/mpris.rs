use std::sync::OnceLock;
use std::thread;

use calloop::channel::Sender;
use log::{error, info};
use mpris::{Event, LoopStatus, PlaybackStatus, PlayerFinder};

static MPRIS: OnceLock<MprisClient> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub identity: String,
    pub active: bool,
    pub status: PlaybackStatus,
    pub title: Option<String>,
    pub artists: Option<Vec<String>>,
    pub volume: Option<f64>,
    pub shuffle: bool,
    pub loop_status: LoopStatus,
}

impl PlayerState {
    pub fn new(identity: String) -> Self {
        Self {
            identity,
            active: true,
            status: PlaybackStatus::Stopped,
            title: None,
            artists: None,
            volume: None,
            shuffle: false,
            loop_status: LoopStatus::None,
        }
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
            }
            _ => {}
        }
    }
}

pub struct MprisClient {
    _sender: Sender<(PlayerState, Event)>,
}

impl MprisClient {
    pub fn init(sender: Sender<(PlayerState, Event)>) {
        MPRIS.get_or_init(|| {
            Self::start_listener(sender.clone());
            Self { _sender: sender }
        });
    }

    fn start_listener(sender: Sender<(PlayerState, Event)>) {
        thread::spawn(move || {
            let finder = match PlayerFinder::new() {
                Ok(finder) => finder,
                Err(err) => {
                    error!("Could not connect to D-Bus: {err}");
                    return;
                }
            };

            let players = match finder.find_all() {
                Ok(players) => players,
                Err(err) => {
                    error!("Could not list players: {err}");
                    return;
                }
            };

            for player in players {
                let identity = player.identity().to_string();
                let sender = sender.clone();

                info!("listening {identity:?}");

                thread::spawn(move || Self::listen_player(identity, sender));
            }
        });
    }

    fn listen_player(identity: String, sender: Sender<(PlayerState, Event)>) {
        let finder = match PlayerFinder::new() {
            Ok(finder) => finder,
            Err(err) => {
                error!("[{identity}] Could not connect to D-Bus: {err}");
                return;
            }
        };

        let player = match finder.find_by_name(&identity) {
            Ok(player) => player,
            Err(err) => {
                error!("[{identity}] Could not reopen player: {err}");
                return;
            }
        };

        let events = match player.events() {
            Ok(events) => events,
            Err(err) => {
                error!("[{identity}] Could not start event stream: {err}");
                return;
            }
        };

        let mut state = PlayerState::new(identity.clone());

        for event in events {
            match event {
                Ok(event) => {
                    state.apply(&event);
                    let _ = sender.send((state.clone(), event));
                    if !state.active {
                        break;
                    }
                }
                Err(err) => {
                    error!("[{identity}] D-Bus error: {err}. Aborting.");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
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
