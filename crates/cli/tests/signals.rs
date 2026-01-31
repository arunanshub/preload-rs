#![forbid(unsafe_code)]

#[cfg(unix)]
mod unix {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::process::{Child, Command, Output, Stdio};
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    #[test]
    fn signals_trigger_reload_dump_and_save() -> io::Result<()> {
        let dir = tempdir()?;
        let config_path = dir.path().join("config.toml");
        write_config(&config_path, 1)?;

        let child = Command::new(env!("CARGO_BIN_EXE_cli"))
            .arg("--config")
            .arg(&config_path)
            .arg("--no-persist")
            .arg("--no-prefetch")
            .arg("-v")
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let pid = Pid::from_raw(child.id() as i32);
        sleep(Duration::from_millis(400));

        kill(pid, Signal::SIGUSR1).ok();
        sleep(Duration::from_millis(400));

        write_config(&config_path, 2)?;
        kill(pid, Signal::SIGHUP).ok();
        sleep(Duration::from_millis(400));

        kill(pid, Signal::SIGUSR1).ok();
        sleep(Duration::from_millis(400));

        kill(pid, Signal::SIGUSR2).ok();
        sleep(Duration::from_millis(500));

        kill(pid, Signal::SIGINT).ok();
        let output = wait_for_output(child)?;

        let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
        combined.push_str(&String::from_utf8_lossy(&output.stderr));

        assert!(combined.contains("current config"));
        assert!(combined.contains("config reloaded"));
        assert!(combined.contains("state saved"));
        assert!(combined.matches("current config").count() >= 2);

        Ok(())
    }

    fn write_config(path: &Path, autosave: u64) -> io::Result<()> {
        let contents = format!(
            "[model]\ncycle = 0\nminsize = 0\n\n[system]\n\
doscan = false\n\
dopredict = false\n\
autosave = {autosave}\n\n\
[persistence]\n\
save_on_shutdown = true\n"
        );
        fs::write(path, contents)
    }

    fn wait_for_output(mut child: Child) -> io::Result<Output> {
        let start = Instant::now();
        loop {
            if let Some(_) = child.try_wait()? {
                break;
            }
            if start.elapsed() > Duration::from_secs(10) {
                let _ = child.kill();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "preload process did not exit",
                ));
            }
            sleep(Duration::from_millis(50));
        }
        child.wait_with_output()
    }
}

#[cfg(not(unix))]
#[test]
fn signals_trigger_reload_dump_and_save() {
    // Signals are only supported in the Unix build.
}
