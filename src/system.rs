//! Status of the machine itself: network, Bluetooth, audio, backlight, battery.

use std::{collections::HashMap, fs, path::Path, thread, time::Duration};

use anti_webui::prelude::UiSender as Sender;
use tokio::{
    process::Command,
    sync::mpsc,
    time::{interval, timeout, MissedTickBehavior},
};
use zbus::{proxy::CacheProperties, zvariant::OwnedValue, Connection};

/// What the user adjusts from the bar: a change made elsewhere shows up this fast.
const SLOW_POLL: Duration = Duration::from_secs(5);
/// `bluetoothctl` waits forever when bluetoothd is down or stuck.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

const POWER_SUPPLIES: &str = "/sys/class/power_supply";
const BLUETOOTH_ADAPTERS: &str = "/sys/class/bluetooth";

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Network {
    #[default]
    Disconnected,
    Wifi(String),
    Wired(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bluetooth {
    pub powered: bool,
    pub connected: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volume {
    pub level: f32,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub percent: u8,
    pub state: BatteryState,
}

/// `Discharging` also covers a battery held at a charge limit, as `Not charging`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryState {
    Discharging,
    Charging,
    Full,
}

/// A power-profiles-daemon profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

impl PowerProfile {
    pub const ALL: [Self; 3] = [Self::PowerSaver, Self::Balanced, Self::Performance];

    fn as_str(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == name)
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_service = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PowerProfiles {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, profile: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;
}

/// `None` means the machine has no such thing, or its tool is unavailable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemStatus {
    pub network: Network,
    pub bluetooth: Bluetooth,
    pub volume: Option<Volume>,
    /// 0 to 1.
    pub brightness: Option<f32>,
    pub battery: Option<Battery>,
    pub power_profile: Option<PowerProfile>,
    /// The profiles this machine offers; performance isn't always among them.
    pub power_profiles: Vec<PowerProfile>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SystemCommand {
    Volume(f32),
    Brightness(f32),
    PowerProfile(PowerProfile),
}

/// ponytail: polls CLI tools because that needs no extra dependencies. Move to
/// PipeWire and D-Bus subscriptions if the poll's process spawns show up on a profile.
pub fn start(sender: Sender<SystemStatus>) -> mpsc::UnboundedSender<SystemCommand> {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

    thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("system: build runtime")
            .block_on(run(sender, cmd_rx));
    });

    cmd_tx
}

async fn run(sender: Sender<SystemStatus>, mut cmd_rx: mpsc::UnboundedReceiver<SystemCommand>) {
    let system_bus = Connection::system()
        .await
        .map_err(|e| log::warn!("system: no system bus: {e}"))
        .ok();

    let mut slow = interval(SLOW_POLL);
    slow.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut status = SystemStatus::default();
    let mut last = None;

    loop {
        tokio::select! {
            Some(cmd) = cmd_rx.recv() => {
                // A slider drag sends far more values than the tools can take, so
                // only the newest of each matters.
                let (mut volume, mut brightness, mut profile) = (None, None, None);
                let mut next = Some(cmd);
                while let Some(cmd) = next {
                    match cmd {
                        SystemCommand::Volume(value) => volume = Some(value),
                        SystemCommand::Brightness(value) => brightness = Some(value),
                        SystemCommand::PowerProfile(value) => profile = Some(value),
                    }
                    next = cmd_rx.try_recv().ok();
                }
                if let Some(value) = volume {
                    let value = format!("{value:.2}");
                    run_tool("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &value]).await;
                }
                if let Some(value) = brightness {
                    let value = format!("{:.0}%", value * 100.0);
                    run_tool("brightnessctl", &["-c", "backlight", "set", &value]).await;
                }
                if let (Some(value), Some(bus)) = (profile, &system_bus) {
                    set_power_profile(bus, value).await;
                    read_power_profiles(&mut status, bus).await;
                }
            }
            _ = slow.tick() => {
                read_slow(&mut status).await;
                if let Some(bus) = &system_bus {
                    read_power_profiles(&mut status, bus).await;
                }
            }
        }

        if last.as_ref() != Some(&status) {
            let _ = sender.send(status.clone());
            last = Some(status.clone());
        }
    }
}

async fn read_slow(status: &mut SystemStatus) {
    status.network = run_tool("nmcli", &["-t", "-f", "TYPE,STATE,CONNECTION", "dev"])
        .await
        .map_or(Network::Disconnected, |output| parse_network(&output));
    // Without an adapter there is nothing to ask, and bluetoothctl would wait until
    // the timeout if bluetoothd never started because of that.
    let has_adapter = fs::read_dir(BLUETOOTH_ADAPTERS).is_ok_and(|mut dir| dir.next().is_some());
    status.bluetooth = if has_adapter {
        let show = run_tool("bluetoothctl", &["show"])
            .await
            .unwrap_or_default();
        let devices = run_tool("bluetoothctl", &["devices", "Connected"])
            .await
            .unwrap_or_default();
        parse_bluetooth(&show, &devices)
    } else {
        Bluetooth::default()
    };
    status.battery = read_battery(Path::new(POWER_SUPPLIES));
}

async fn power_profiles_proxy(bus: &Connection) -> Option<PowerProfilesProxy<'_>> {
    PowerProfilesProxy::builder(bus)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()
}

async fn read_power_profiles(status: &mut SystemStatus, bus: &Connection) {
    let Some(proxy) = power_profiles_proxy(bus).await else {
        return;
    };
    let active = timeout(COMMAND_TIMEOUT, proxy.active_profile()).await;
    let profiles = timeout(COMMAND_TIMEOUT, proxy.profiles()).await;
    status.power_profile = active
        .ok()
        .and_then(Result::ok)
        .and_then(|name| PowerProfile::parse(&name));
    status.power_profiles = profiles
        .ok()
        .and_then(Result::ok)
        .map_or_else(Vec::new, |profiles| parse_power_profiles(&profiles));
}

fn parse_power_profiles(profiles: &[HashMap<String, OwnedValue>]) -> Vec<PowerProfile> {
    profiles
        .iter()
        .filter_map(|profile| <&str>::try_from(profile.get("Profile")?).ok())
        .filter_map(PowerProfile::parse)
        .collect()
}

async fn set_power_profile(bus: &Connection, profile: PowerProfile) {
    let Some(proxy) = power_profiles_proxy(bus).await else {
        log::warn!("system: power profiles are unavailable");
        return;
    };
    match timeout(COMMAND_TIMEOUT, proxy.set_active_profile(profile.as_str())).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::warn!("system: could not set power profile: {e}"),
        Err(_) => log::warn!("system: setting the power profile timed out"),
    }
}

/// Stdout of a tool that succeeded in time.
async fn run_tool(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .kill_on_drop(true)
        .output();
    match timeout(COMMAND_TIMEOUT, output).await {
        Ok(Ok(output)) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(Ok(output)) => {
            log::debug!(
                "system: `{program}` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            None
        }
        Ok(Err(e)) => {
            log::debug!("system: could not run `{program}`: {e}");
            None
        }
        Err(_) => {
            log::debug!("system: `{program}` timed out");
            None
        }
    }
}

/// Lines of `TYPE:STATE:CONNECTION`, with colons in the name escaped. Wi-Fi wins
/// over a cable if both are up, since that is what the icon is about.
fn parse_network(output: &str) -> Network {
    let mut wired = None;
    for line in output.lines() {
        let mut fields = line.splitn(3, ':');
        let (Some(kind), Some("connected"), Some(name)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let name = name.replace("\\:", ":");
        match kind {
            "wifi" => return Network::Wifi(name),
            "ethernet" => wired = Some(name),
            _ => {}
        }
    }
    wired.map_or(Network::Disconnected, Network::Wired)
}

/// `show` has a `Powered: yes` line, `devices` has `Device <address> <name>` lines.
fn parse_bluetooth(show: &str, devices: &str) -> Bluetooth {
    Bluetooth {
        powered: show.lines().any(|line| line.trim() == "Powered: yes"),
        connected: devices
            .lines()
            .filter_map(|line| line.strip_prefix("Device "))
            .filter_map(|line| line.split_once(' '))
            .map(|(_, name)| name.to_string())
            .collect(),
    }
}

/// The machine's own battery: devices such as a wireless mouse also list batteries
/// here, marked with `scope=Device`.
fn read_battery(power_supplies: &Path) -> Option<Battery> {
    fs::read_dir(power_supplies)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| read_trimmed(&path.join("type")).as_deref() == Some("Battery"))
        .filter(|path| read_trimmed(&path.join("scope")).as_deref() != Some("Device"))
        .find_map(|path| {
            Some(Battery {
                percent: read_trimmed(&path.join("capacity"))?.parse().ok()?,
                state: match read_trimmed(&path.join("status")).as_deref() {
                    Some("Charging") => BatteryState::Charging,
                    Some("Full") => BatteryState::Full,
                    _ => BatteryState::Discharging,
                },
            })
        })
}

fn read_trimmed(path: &Path) -> Option<String> {
    Some(fs::read_to_string(path).ok()?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_power_profiles() {
        let profile = |name: &str| {
            let name = OwnedValue::try_from(zbus::zvariant::Value::from(name)).unwrap();
            HashMap::from([("Profile".to_string(), name)])
        };
        let profiles = [
            profile("power-saver"),
            profile("balanced"),
            profile("future"),
        ];

        assert_eq!(
            parse_power_profiles(&profiles),
            [PowerProfile::PowerSaver, PowerProfile::Balanced]
        );
        assert_eq!(
            PowerProfile::parse("performance"),
            Some(PowerProfile::Performance)
        );
        assert_eq!(PowerProfile::parse(""), None);
    }

    #[tokio::test]
    #[ignore = "reads the real system bus"]
    async fn reads_live_power_profiles() {
        let bus = Connection::system().await.unwrap();
        let mut status = SystemStatus::default();
        read_power_profiles(&mut status, &bus).await;
        println!(
            "live: {:?} of {:?}",
            status.power_profile, status.power_profiles
        );
    }

    #[test]
    fn parses_network() {
        let wired = "ethernet:connected:有線接続 1\nloopback:connected (externally):lo\n";
        assert_eq!(parse_network(wired), Network::Wired("有線接続 1".into()));

        let both = "ethernet:connected:cable\nwifi:connected:My\\:Net\n";
        assert_eq!(parse_network(both), Network::Wifi("My:Net".into()));

        let down = "wifi:disconnected:\nethernet:unavailable:\n";
        assert_eq!(parse_network(down), Network::Disconnected);
    }

    #[test]
    fn parses_bluetooth() {
        let show = "Controller AA:BB\n\tName: pc\n\tPowered: yes\n";
        let devices = "Device AA:BB:CC:DD:EE:FF My Headphones\n";
        assert_eq!(
            parse_bluetooth(show, devices),
            Bluetooth {
                powered: true,
                connected: vec!["My Headphones".into()],
            }
        );
        assert_eq!(parse_bluetooth("", ""), Bluetooth::default());
    }

    fn write_files(dir: &Path, files: &[(&str, &str)]) {
        fs::create_dir_all(dir).unwrap();
        for (name, content) in files {
            fs::write(dir.join(name), content).unwrap();
        }
    }

    #[test]
    fn reads_the_machines_battery_and_backlight() {
        let root = std::env::temp_dir().join(format!("my_shell_system_{}", std::process::id()));
        let supplies = root.join("supplies");
        let mouse = [
            ("type", "Battery\n"),
            ("scope", "Device\n"),
            ("capacity", "63\n"),
            ("status", "Discharging\n"),
        ];
        write_files(&supplies.join("hidpp_battery_0"), &mouse);
        write_files(&supplies.join("AC"), &[("type", "Mains\n")]);
        assert_eq!(read_battery(&supplies), None, "a mouse is not our battery");

        let own = [
            ("type", "Battery\n"),
            ("capacity", "80\n"),
            ("status", "Charging\n"),
        ];
        write_files(&supplies.join("BAT0"), &own);
        assert_eq!(
            read_battery(&supplies),
            Some(Battery {
                percent: 80,
                state: BatteryState::Charging
            })
        );

        write_files(&supplies.join("BAT0"), &[("status", "Full\n")]);
        let full = read_battery(&supplies).unwrap();
        assert_eq!(full.state, BatteryState::Full);
        write_files(&supplies.join("BAT0"), &[("status", "Not charging\n")]);
        let limited = read_battery(&supplies).unwrap();
        assert_eq!(limited.state, BatteryState::Discharging);

        fs::remove_dir_all(&root).unwrap();
    }
}
