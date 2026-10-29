use anyhow::{Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use linux_flight_recorder::{
    collect::{self, LinuxSources},
    diff,
    model::Config,
    privacy, report, runner,
    storage::{self, Store},
};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Parser)]
#[command(
    name = "lfr",
    version,
    about = "Local Linux diagnostic snapshots; no automatic repairs",
    after_help = "Examples:\n  lfr snapshot create --label before-update\n  lfr compare BEFORE_ID AFTER_ID --format json\n  lfr report BEFORE_ID AFTER_ID --symptom wifi --output report.md\n  lfr export ID --preview\nExit codes: 0 success, 1 failure, 2 usage/config, 3 partial snapshot, 130 interrupted."
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Private absolute store path (0700); user/system stores never merged"
    )]
    store: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help = "Use /var/lib/lfr; root required; never invokes sudo"
    )]
    system: bool,
    #[arg(
        long,
        global = true,
        help = "Explicit JSON configuration (unknown keys rejected)"
    )]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}
#[derive(Subcommand)]
enum Cmd {
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCmd,
    },
    Compare {
        before: String,
        after: String,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
    Report {
        before: String,
        after: String,
        #[arg(long)]
        symptom: Option<String>,
        #[arg(long)]
        output: PathBuf,
    },
    Export {
        id: String,
        #[arg(long, required_unless_present = "preview")]
        output: Option<PathBuf>,
        #[arg(long)]
        preview: bool,
    },
    Doctor,
    Config {
        #[command(subcommand)]
        command: ConfigCmd,
    },
    Gc {
        #[arg(long)]
        dry_run: bool,
    },
}
#[derive(Subcommand)]
enum ConfigCmd {
    Validate,
}
#[derive(Subcommand)]
enum SnapshotCmd {
    Create {
        #[arg(long, default_value = "")]
        label: String,
        #[arg(long, value_enum, default_value = "manual")]
        source: Source,
    },
    List,
    Show {
        id: String,
    },
    Delete {
        id: String,
        #[arg(long)]
        allow_pinned: bool,
    },
    Pin {
        id: String,
    },
}
#[derive(Clone, ValueEnum)]
enum Source {
    Manual,
    Timer,
    Boot,
    PackageEvent,
}
#[derive(Clone, ValueEnum)]
enum Format {
    Text,
    Json,
}
fn print_json(v: &impl serde::Serialize) -> Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, v)?;
    out.write_all(b"\n")?;
    Ok(())
}
fn default_store(system: bool) -> Result<PathBuf> {
    if system {
        return Ok("/var/lib/lfr".into());
    }
    if let Some(p) = std::env::var_os("XDG_STATE_HOME") {
        let p = PathBuf::from(p);
        ensure!(p.is_absolute(), "XDG_STATE_HOME must be absolute");
        return Ok(p.join("lfr"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| anyhow::anyhow!("HOME unavailable; specify --store"))?;
    let p = PathBuf::from(home);
    ensure!(p.is_absolute(), "HOME must be absolute");
    Ok(p.join(".local/state/lfr"))
}
fn id() -> Result<String> {
    let mut b = [0_u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b.iter().map(|b| format!("{b:02x}")).collect())
}
fn run(cli: Cli, c: Config, cancel: Arc<AtomicBool>) -> Result<i32> {
    if let Cmd::Config { .. } = &cli.command {
        print_json(&serde_json :: json ! ({ "valid" : true , "effective" : c }))?;
        return Ok(0);
    }
    if let Cmd::Doctor = &cli.command {
        let tools = ["rpm", "dpkg-query", "pacman", "systemctl", "journalctl"]
            .map(|n| (n, runner::executable(n).is_some()))
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        print_json(
            &serde_json :: json ! ({ "schema_version" : 1 , "tools" : tools , "systemd_runtime" : std :: path :: Path :: new ("/run/systemd/system") . is_dir () , "uid" : unsafe { libc :: geteuid () } , "default_store" : default_store (cli . system) ?, "notes" : ["Presence of a tool does not prove source access or distribution support." , "No settings are changed."] }),
        )?;
        return Ok(0);
    }
    let root = unsafe { libc::geteuid() } == 0;
    ensure!(
        !root || cli.system,
        "root collection/storage requires explicit --system; user store must not be mixed with root data"
    );
    ensure!(
        !cli.system || root,
        "--system requires root; lfr never invokes sudo"
    );
    let path = match cli.store {
        Some(p) => p,
        None => default_store(cli.system)?,
    };
    let mut store = Store::open(&path)?;
    match cli.command {
        Cmd::Snapshot { command } => match command {
            SnapshotCmd::Create { label, source } => {
                ensure!(label.len() <= 256, "label exceeds 256 bytes");
                let run = runner::SystemRunner {
                    config: c.clone(),
                    cancelled: cancel.clone(),
                };
                let sources = LinuxSources { root: "/".into() };
                let source = match source {
                    Source::Manual => "manual",
                    Source::Timer => "timer",
                    Source::Boot => "boot",
                    Source::PackageEvent => "package-event",
                };
                let s = collect::collect(
                    &sources,
                    &run,
                    &c,
                    chrono::Utc::now(),
                    id()?,
                    label,
                    source.into(),
                );
                if cancel.load(Ordering::Relaxed) {
                    return Ok(130);
                }
                store.save(&s, &c)?;
                print_json(
                    &serde_json :: json ! ({ "id" : s . id , "completeness" : s . completeness , "started_at" : s . started_at , "finished_at" : s . finished_at }),
                )?;
                return Ok(
                    if s.completeness == linux_flight_recorder::model::Status::Ok {
                        0
                    } else {
                        3
                    },
                );
            }
            SnapshotCmd::List => print_json(&store.list()?)?,
            SnapshotCmd::Show { id } => print_json(&store.load(&id)?)?,
            SnapshotCmd::Delete { id, allow_pinned } => {
                store.delete(&id, allow_pinned)?;
                print_json(&serde_json :: json ! ({ "deleted" : id }))?;
            }
            SnapshotCmd::Pin { id } => {
                store.pin(&id)?;
                print_json(&serde_json :: json ! ({ "pinned" : id }))?;
            }
        },
        Cmd::Compare {
            before,
            after,
            format,
        } => {
            let a = store.load(&before)?;
            let b = store.load(&after)?;
            let d = diff::compare(&a, &b)?;
            match format {
                Format::Json => print_json(&d)?,
                Format::Text => io::stdout().write_all(report::terminal(&d).as_bytes())?,
            }
        }
        Cmd::Report {
            before,
            after,
            symptom,
            output,
        } => {
            let a = store.load(&before)?;
            let b = store.load(&after)?;
            let d = diff::compare(&a, &b)?;
            storage::write_bytes(
                &output,
                report::markdown(&a, &b, &d, symptom.as_deref()).as_bytes(),
            )?;
            eprintln!("Report written; review private data before sharing.");
        }
        Cmd::Export {
            id,
            output,
            preview,
        } => {
            let s = store.load(&id)?;
            if preview {
                print_json(&report::preview(&s))?;
            } else {
                report::export(
                    &s,
                    output
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("output required"))?,
                )?;
                eprintln!("Bundle written; manual privacy review required.");
            }
        }
        Cmd::Gc { dry_run } => print_json(&store.gc(&c, chrono::Utc::now(), dry_run)?)?,
        Cmd::Doctor | Cmd::Config { .. } => unreachable!(),
    }
    Ok(if cancel.load(Ordering::Relaxed) {
        130
    } else {
        0
    })
}
fn main() {
    unsafe {
        libc::umask(0o077);
    }
    let cli = Cli::parse();
    let c = match storage::read_config(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "lfr: invalid configuration: {}",
                privacy::visible(&e.to_string())
            );
            std::process::exit(2);
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        if let Err(e) = signal_hook::flag::register(sig, cancel.clone()) {
            eprintln!("lfr: cannot install signal handler: {e}");
            std::process::exit(1);
        }
    }
    let code = match run(cli, c, cancel.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "lfr: {}",
                privacy::visible(&privacy::redact(&format!("{e:#}"), &[]))
            );
            if cancel.load(Ordering::Relaxed) {
                130
            } else {
                1
            }
        }
    };
    std::process::exit(code);
}
