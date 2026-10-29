use crate::model::{Config, Status};
use std::{
    fs,
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::{fs::MetadataExt, process::CommandExt},
    },
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Debug, Clone)]
pub struct Output {
    pub status: Status,
    pub stdout: Vec<u8>,
    pub diagnostic: String,
}
pub trait Runner {
    fn run(&self, program: &str, args: &[&str]) -> Output;
}
pub struct SystemRunner {
    pub config: Config,
    pub cancelled: Arc<AtomicBool>,
}
pub fn executable(name: &str) -> Option<std::path::PathBuf> {
    if !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return None;
    }
    ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        .iter()
        .map(|d| std::path::Path::new(d).join(name))
        .find(|p| {
            fs::metadata(p).is_ok_and(|m| {
                m.is_file() && m.uid() == 0 && m.mode() & 0o022 == 0 && m.mode() & 0o111 != 0
            })
        })
}
impl Runner for SystemRunner {
    fn run(&self, name: &str, args: &[&str]) -> Output {
        let Some(path) = executable(name) else {
            return Output {
                status: Status::Unavailable,
                stdout: vec![],
                diagnostic: format!("trusted executable {name} unavailable"),
            };
        };
        bounded_command(
            &path,
            args,
            Duration::from_millis(self.config.command_timeout_ms),
            self.config.command_output_bytes,
            &self.cancelled,
        )
    }
}
fn nonblock(fd: i32) -> io::Result<()> {
    unsafe {
        if libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
pub fn bounded_command(
    path: &std::path::Path,
    args: &[&str],
    timeout: Duration,
    cap: usize,
    cancel: &AtomicBool,
) -> Output {
    let mut command = Command::new(path);
    command
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("PATH", "/usr/bin:/bin")
        .env("TZ", "UTC")
        .env("SYSTEMD_COLORS", "0")
        .env("SYSTEMD_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    unsafe {
        command.pre_exec(|| {
            for (resource, limit) in [
                (libc::RLIMIT_AS, 256 * 1024 * 1024),
                (libc::RLIMIT_CPU, 30),
                (libc::RLIMIT_CORE, 0),
            ] {
                let value = libc::rlimit {
                    rlim_cur: limit,
                    rlim_max: limit,
                };
                if libc::setrlimit(resource, &value) != 0 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Output {
                status: if e.kind() == io::ErrorKind::PermissionDenied {
                    Status::PermissionDenied
                } else {
                    Status::Failed
                },
                stdout: vec![],
                diagnostic: e.to_string(),
            };
        }
    };
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let start = Instant::now();
    let mut status = Status::Ok;
    let mut reason = String::new();
    let setup = nonblock(stdout.as_raw_fd()).and_then(|_| nonblock(stderr.as_raw_fd()));
    if let Err(e) = setup {
        status = Status::Failed;
        reason = e.to_string();
    }
    let mut done = [false, false];
    while status == Status::Ok {
        if cancel.load(Ordering::Relaxed) {
            status = Status::Failed;
            reason = "interrupted".into();
            break;
        }
        if start.elapsed() >= timeout {
            status = Status::Timeout;
            reason = "command deadline exceeded".into();
            break;
        }
        for (i, stream) in [&mut stdout as &mut dyn Read, &mut stderr as &mut dyn Read]
            .into_iter()
            .enumerate()
        {
            if done[i] {
                continue;
            }
            let mut buf = [0_u8; 8192];
            match stream.read(&mut buf) {
                Ok(0) => done[i] = true,
                Ok(n) => {
                    let remain = cap.saturating_sub(out.len() + err.len());
                    let target = if i == 0 { &mut out } else { &mut err };
                    target.extend_from_slice(&buf[..n.min(remain)]);
                    if n > remain {
                        status = Status::Partial;
                        reason = "combined output byte limit exceeded".into();
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    status = Status::Failed;
                    reason = e.to_string();
                }
            }
        }
        match child.try_wait() {
            Ok(Some(exit)) if done.iter().all(|b| *b) => {
                if !exit.success() {
                    status = Status::Failed;
                    reason = format!("exit {exit}");
                }
                break;
            }
            Err(e) => {
                status = Status::Failed;
                reason = e.to_string();
                break;
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.wait();
    let err = String::from_utf8_lossy(&err);
    if !err.trim().is_empty() {
        if status == Status::Ok {
            status = Status::Partial;
        }
        if err.to_ascii_lowercase().contains("permission denied")
            || err.contains("not seeing messages")
        {
            status = Status::PermissionDenied;
        }
        reason.push_str(": ");
        reason.push_str(&crate::privacy::visible(&crate::privacy::redact(&err, &[])));
    }
    Output {
        status,
        stdout: out,
        diagnostic: reason,
    }
}
