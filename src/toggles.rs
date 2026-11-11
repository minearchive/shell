//! What the toolbox's toggles hold on to while they are on. Both end with the bar: the inhibitor
//! is a file descriptor, and the night light's process is told when the bar goes away.

use std::{
    os::{fd::OwnedFd, unix::process::CommandExt},
    process::{Child, Command},
};

/// The colour temperature the night light holds the screen at, in kelvin.
const NIGHT_LIGHT_KELVIN: u32 = 4000;

/// Keeps the screen from blanking while held: logind's idle inhibitor, which hypridle and
/// swayidle respect. Closing the descriptor releases it.
pub fn inhibit_idle() -> zbus::Result<OwnedFd> {
    zbus::block_on(async {
        let reply = zbus::Connection::system()
            .await?
            .call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "Inhibit",
                &("idle", "my_shell", "画面を消さない", "block"),
            )
            .await?;
        let fd: zbus::zvariant::OwnedFd = reply.body().deserialize()?;
        Ok(fd.into())
    })
}

/// Holds every screen at a warm colour until stopped with [`stop`].
pub fn night_light() -> std::io::Result<Child> {
    let mut command = Command::new("gammastep");
    command.args(["-O", &NIGHT_LIGHT_KELVIN.to_string()]);
    // SAFETY: prctl is async-signal-safe and touches nothing of this process's.
    unsafe {
        // Without this a crashed bar would leave the screen warm, with nothing to turn it off.
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    command.spawn()
}

/// Asks `child` to stop, so that it restores the screen's colours, and reaps it.
pub fn stop(mut child: Child) {
    // SAFETY: kill only sends a signal; the pid is ours until we wait on it.
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    std::thread::spawn(move || child.wait());
}
