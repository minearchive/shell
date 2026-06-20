use mpris::Player;
use mpris::PlayerFinder;

pub struct MprisClient {
    players: Vec<Player>,
}

impl MprisClient {
    pub fn new() -> Self {
        let player_finder = PlayerFinder::new().expect("Failed to connect mpris via dbus");

        let players = player_finder.find_all().unwrap_or(Vec::new());

        Self { players }
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
