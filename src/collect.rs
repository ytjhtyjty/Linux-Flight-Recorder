use crate::{
    model::{Config, ResultSet, Row, SCHEMA, Snapshot, Status, row},
    privacy,
    runner::{Output, Runner},
};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::Instant,
};
pub trait Sources {
    fn read(&self, path: &str, cap: usize) -> io::Result<Vec<u8>>;
    fn entries(&self, path: &str, cap: usize) -> io::Result<Vec<String>>;
    fn link(&self, path: &str) -> io::Result<String>;
}
pub struct LinuxSources {
    pub root: PathBuf,
}
impl Sources for LinuxSources {
    fn read(&self, path: &str, cap: usize) -> io::Result<Vec<u8>> {
        let mut b = vec![];
        fs::File::open(self.root.join(path.trim_start_matches('/')))?
            .take(cap as u64 + 1)
            .read_to_end(&mut b)?;
        if b.len() > cap {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "source byte budget exceeded",
            ));
        }
        Ok(b)
    }
    fn entries(&self, path: &str, cap: usize) -> io::Result<Vec<String>> {
        let mut v = vec![];
        for e in fs::read_dir(self.root.join(path.trim_start_matches('/')))? {
            let name = e?
                .file_name()
                .into_string()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 entry"))?;
            if v.len() == cap {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "entry budget exceeded",
                ));
            }
            v.push(name);
        }
        v.sort();
        Ok(v)
    }
    fn link(&self, path: &str) -> io::Result<String> {
        let p = self.root.join(path.trim_start_matches('/'));
        let p = fs::canonicalize(p)?;
        let relative = p.strip_prefix(&self.root).unwrap_or(&p);
        Ok(relative.to_string_lossy().into_owned())
    }
}
fn text(s: &dyn Sources, p: &str, c: &Config) -> io::Result<String> {
    String::from_utf8(s.read(p, c.source_bytes)?)
        .map(|s| s.trim().into())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 source"))
}
fn error_status(e: &io::Error) -> Status {
    match e.kind() {
        io::ErrorKind::PermissionDenied => Status::PermissionDenied,
        io::ErrorKind::NotFound => Status::Unavailable,
        _ => Status::Failed,
    }
}
fn attr(
    s: &dyn Sources,
    base: &str,
    name: &str,
    c: &Config,
    r: &mut Row,
    set: &mut ResultSet,
    required: bool,
) {
    match text(s, &format!("{base}/{name}"), c) {
        Ok(v) => {
            r.insert(name.replace('/', "_"), v);
        }
        Err(e) if required || e.kind() != io::ErrorKind::NotFound => {
            set.status = Status::Partial;
            if set.limitations.len() < 32 {
                set.limitations
                    .push(format!("{base}/{name}: {:?}", e.kind()));
            }
        }
        _ => {}
    }
}
fn device_link(
    s: &dyn Sources,
    base: &str,
    suffix: &str,
    key: &str,
    r: &mut Row,
    set: &mut ResultSet,
) {
    match s.link(&format!("{base}/{suffix}")) {
        Ok(v) => {
            r.insert(
                key.into(),
                if key == "device" {
                    v
                } else {
                    Path::new(&v)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                },
            );
        }
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            set.status = Status::Partial;
            if set.limitations.len() < 32 {
                set.limitations
                    .push(format!("{base}/{suffix}: {:?}", e.kind()));
            }
        }
        _ => {}
    }
}
fn devices(s: &dyn Sources, c: &Config, kind: &str, base: &str) -> ResultSet {
    let start = Instant::now();
    let mut set = ResultSet::empty(Status::Ok, base, "");
    let mut collected_bytes = 0_usize;
    let names = match s.entries(base, c.source_entries) {
        Ok(v) => v,
        Err(e) => return ResultSet::empty(error_status(&e), base, &e.to_string()),
    };
    for name in names {
        if start.elapsed().as_millis() >= c.source_timeout_ms as u128 {
            set.status = Status::Timeout;
            set.limitations.push("source deadline exceeded".into());
            break;
        }
        if kind == "drm" && !(name.starts_with("card") || name.starts_with("renderD")) {
            continue;
        }
        if kind == "audio"
            && !(name.starts_with("card") && name[4..].chars().all(|x| x.is_ascii_digit()))
        {
            continue;
        }
        let path = format!("{base}/{name}");
        let mut r = row(&[("location", &name)]);
        let fields: &[&str] = match kind {
            "pci" => &[
                "vendor",
                "device",
                "class",
                "subsystem_vendor",
                "subsystem_device",
            ],
            "usb" => &["idVendor", "idProduct", "bDeviceClass", "speed"],
            "drm" => {
                if name.contains('-') {
                    &["status", "modes"]
                } else {
                    &[]
                }
            }
            "network" => &["operstate", "flags", "type"],
            "rfkill" => &["type", "soft", "hard"],
            "audio" => &["id"],
            _ => &[],
        };
        let usb_interface = kind == "usb" && name.contains(':');
        let attribute_base = if usb_interface {
            format!("{base}/{}", name.split(':').next().unwrap_or(""))
        } else {
            path.clone()
        };
        if usb_interface {
            r.insert(
                "interface".into(),
                name.split_once(':').map(|(_, v)| v).unwrap_or("").into(),
            );
        }
        for field in fields {
            attr(s, &attribute_base, field, c, &mut r, &mut set, true);
        }
        device_link(s, &path, "driver", "driver", &mut r, &mut set);
        device_link(s, &path, "driver/module", "module", &mut r, &mut set);
        if kind != "pci" && kind != "usb" {
            device_link(s, &path, "device", "device", &mut r, &mut set);
            device_link(s, &path, "device/driver", "driver", &mut r, &mut set);
        }
        if kind == "drm" && name.contains('-') {
            let card = name.split('-').next().unwrap_or("");
            device_link(
                s,
                &format!("{base}/{card}"),
                "device",
                "device",
                &mut r,
                &mut set,
            );
            device_link(
                s,
                &format!("{base}/{card}"),
                "device/driver",
                "driver",
                &mut r,
                &mut set,
            );
        }
        if ["network", "drm", "audio"].contains(&kind) {
            let hwbase = if kind == "drm" && name.contains('-') {
                format!("{base}/{}/device", name.split('-').next().unwrap_or(""))
            } else {
                format!("{path}/device")
            };
            for field in ["vendor", "device", "subsystem_vendor", "subsystem_device"] {
                if let Ok(v) = text(s, &format!("{hwbase}/{field}"), c) {
                    r.insert(format!("hardware_{field}"), v);
                }
            }
            if kind == "drm" {
                r.insert(
                    "connector".into(),
                    name.split_once('-')
                        .map(|(_, s)| s)
                        .unwrap_or("device")
                        .into(),
                );
            }
        }
        if kind == "network"
            && let Some(flags) = r
                .get("flags")
                .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        {
            r.insert(
                "admin_state".into(),
                if flags & 1 != 0 { "up" } else { "down" }.into(),
            );
        }
        let bytes = r.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>();
        if collected_bytes.saturating_add(bytes) > 2 * 1024 * 1024 {
            set.status = Status::Partial;
            set.limitations
                .push("collector data budget exceeded (2 MiB)".into());
            break;
        }
        collected_bytes += bytes;
        set.records.push(r);
    }
    set.duration_ms = start.elapsed().as_millis() as u64;
    set
}
pub fn parse_packages(adapter: &str, data: &[u8]) -> Result<Vec<Row>, String> {
    let text = std::str::from_utf8(data).map_err(|_| "non-UTF8 package output")?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err("truncated package output".into());
    }
    let mut rows = vec![];
    for line in text.lines() {
        let f: Vec<&str> = if adapter == "pacman" {
            line.split_whitespace().collect()
        } else {
            line.split('\t').collect()
        };
        let (name, version, arch) = match adapter {
            "dpkg" if f.len() == 4 => {
                if f[0] != "installed" {
                    continue;
                }
                (f[1], f[2], f[3])
            }
            "rpm" if f.len() == 3 => (f[0], f[1], f[2]),
            "pacman" if f.len() == 2 => (f[0], f[1], "unavailable"),
            _ => return Err(format!("malformed {adapter} inventory")),
        };
        if name.is_empty() || version.is_empty() || name.chars().any(char::is_control) {
            return Err("invalid package record".into());
        }
        if rows.len() >= 32768 {
            return Err("package record cap exceeded".into());
        }
        rows.push(row(&[
            ("name", name),
            ("version", version),
            ("architecture", arch),
            ("adapter", adapter),
            (
                "stack_group",
                if name.contains("firmware") {
                    "firmware"
                } else if name.starts_with("linux") || name.starts_with("kernel") {
                    "kernel"
                } else if ["mesa", "nvidia", "vulkan", "libdrm", "xorg"]
                    .iter()
                    .any(|x| name.contains(x))
                {
                    "graphics"
                } else {
                    "other"
                },
            ),
        ]));
    }
    rows.sort();
    Ok(rows)
}
fn packages(run: &dyn Runner, distro: &str) -> ResultSet {
    let (adapter, program, args): (&str, &str, Vec<&str>) = match distro {
        "debian" | "ubuntu" => (
            "dpkg",
            "dpkg-query",
            vec![
                "-W",
                "-f=${db:Status-Status}\t${Package}\t${Version}\t${Architecture}\n",
            ],
        ),
        "arch" | "manjaro" => ("pacman", "pacman", vec!["-Q"]),
        "fedora" | "amzn" | "rhel" | "centos" => (
            "rpm",
            "rpm",
            vec![
                "-qa",
                "--qf",
                "%{NAME}\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\t%{ARCH}\n",
            ],
        ),
        _ => {
            return ResultSet::empty(
                Status::Unavailable,
                "package adapter",
                "unknown distribution; no guessed package manager",
            );
        }
    };
    let start = Instant::now();
    let o = run.run(program, &args);
    let mut set = ResultSet::empty(o.status, program, &o.diagnostic);
    match parse_packages(adapter, &o.stdout) {
        Ok(rows) => set.records = rows,
        Err(e) => {
            set.status = Status::Failed;
            set.limitations.push(e);
        }
    }
    if adapter == "pacman" {
        set.limitations.push(
            "package architecture unavailable from pacman -Q; no version direction inferred".into(),
        );
    }
    set.duration_ms = start.elapsed().as_millis() as u64;
    set
}
pub fn parse_journal(data: &[u8], max: usize) -> Result<Vec<Row>, String> {
    let t = std::str::from_utf8(data).map_err(|_| "non-UTF8 journal framing")?;
    if !t.is_empty() && !t.ends_with('\n') {
        return Err("truncated journal JSON line".into());
    }
    let mut rows = vec![];
    for line in t.lines() {
        if rows.len() >= max {
            return Err("journal record limit".into());
        }
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|_| "malformed journal JSON")?;
        let mut r = Row::new();
        for (field, key) in [
            ("MESSAGE", "message"),
            ("_SYSTEMD_UNIT", "unit"),
            ("_TRANSPORT", "transport"),
            ("PRIORITY", "priority"),
            ("__CURSOR", "cursor"),
            ("__REALTIME_TIMESTAMP", "timestamp"),
            ("_BOOT_ID", "boot_id"),
        ] {
            if let Some(value) = v.get(field) {
                let val = if let Some(s) = value.as_str() {
                    s.to_owned()
                } else {
                    "[non-scalar journal field omitted]".into()
                };
                r.insert(key.into(), val);
            }
        }
        if !r.contains_key("message") {
            return Err("journal MESSAGE missing".into());
        }
        rows.push(r);
    }
    Ok(rows)
}
fn journal(run: &dyn Runner, c: &Config, now: chrono::DateTime<chrono::Utc>) -> ResultSet {
    let start = Instant::now();
    let from = (now - chrono::Duration::seconds(c.journal_window_seconds as i64))
        .timestamp()
        .to_string();
    let until = now.timestamp().to_string();
    let n = c.journal_records.to_string();
    let Output {
        status,
        stdout,
        diagnostic,
    } = run.run(
        "journalctl",
        &[
            "--no-pager",
            "--system",
            "--boot=0",
            "--priority=0..3",
            "--output=json",
            "--output-fields=MESSAGE,_SYSTEMD_UNIT,_TRANSPORT,PRIORITY",
            &format!("--since=@{from}"),
            &format!("--until=@{until}"),
            &format!("--lines={n}"),
        ],
    );
    let mut set = ResultSet::empty(
        status,
        "journalctl system/current-boot priority 0..3",
        &diagnostic,
    );
    set.limitations.push(format!(
        "window_unix={from}..{until}; max_records={n}; no frequency or novelty claims"
    ));
    match parse_journal(&stdout, c.journal_records) {
        Ok(rows) => set.records = rows,
        Err(e) => {
            set.status = Status::Failed;
            set.limitations.push(e);
        }
    }
    if set.status == Status::Failed
        && stdout.is_empty()
        && diagnostic.contains("No journal files were opened due to insufficient permissions.")
    {
        set.status = Status::PermissionDenied;
    }
    if set.status == Status::Ok {
        set.status = Status::Partial;
        set . limitations . push ("journal persistence, full source access and complete window coverage not proven; empty does not mean no errors" . into ()) ;
    }
    set.duration_ms = start.elapsed().as_millis() as u64;
    set
}
fn services(run: &dyn Runner) -> ResultSet {
    let start = Instant::now();
    let o = run.run(
        "systemctl",
        &[
            "--system",
            "--no-pager",
            "--plain",
            "--no-legend",
            "--all",
            "--state=failed",
            "list-units",
        ],
    );
    let mut set = ResultSet::empty(o.status, "systemctl failed system units", &o.diagnostic);
    match std::str::from_utf8(&o.stdout) {
        Ok(t) => {
            for l in t.lines() {
                let f: Vec<_> = l.split_whitespace().collect();
                if f.len() >= 4 {
                    set.records.push(row(&[
                        ("name", f[0]),
                        ("load", f[1]),
                        ("active", f[2]),
                        ("sub", f[3]),
                    ]));
                } else if !l.trim().is_empty() {
                    set.status = Status::Partial;
                    set.limitations.push("unparsed systemctl line".into());
                }
            }
        }
        Err(_) => {
            set.status = Status::Failed;
            set.limitations.push("non-UTF8 systemctl output".into());
        }
    }
    set . limitations . push ("This collector covers failed system units; selected properties are separate. User audio sessions are not queried." . into ()) ;
    set.records.sort();
    set.duration_ms = start.elapsed().as_millis() as u64;
    set
}
pub fn parse_service_properties(data: &[u8]) -> Result<Vec<Row>, String> {
    let text = std::str::from_utf8(data).map_err(|_| "non-UTF8 service properties")?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err("truncated service properties".into());
    }
    let mut rows = vec![];
    let mut current = Row::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if !current.is_empty() {
                if !["name", "load", "active", "sub"]
                    .iter()
                    .all(|k| current.contains_key(*k))
                {
                    return Err("missing selected service properties".into());
                }
                rows.push(std::mem::take(&mut current));
            }
            continue;
        }
        let (key, value) = line.split_once('=').ok_or("malformed service property")?;
        let key = match key {
            "Id" => "name",
            "LoadState" => "load",
            "ActiveState" => "active",
            "SubState" => "sub",
            _ => continue,
        };
        if current.insert(key.into(), value.into()).is_some() {
            return Err("duplicate selected service property".into());
        }
    }
    rows.sort();
    Ok(rows)
}
fn selected_services(run: &dyn Runner, c: &Config) -> ResultSet {
    let mut set = ResultSet::empty(
        Status::Ok,
        "systemctl show selected system service properties",
        "",
    );
    let mut names = c.selected_services.clone();
    names.sort();
    set.sources
        .push(format!("configured_units={}", names.join(",")));
    if c.selected_services.is_empty() {
        set.limitations
            .push("No selected system services configured; user sessions never queried.".into());
        return set;
    }
    let mut args = vec![
        "--system",
        "--no-pager",
        "show",
        "--property=Id,LoadState,ActiveState,SubState",
        "--",
    ];
    args.extend(c.selected_services.iter().map(String::as_str));
    let start = Instant::now();
    let o = run.run("systemctl", &args);
    set.status = o.status;
    if !o.diagnostic.is_empty() {
        set.limitations.push(o.diagnostic);
    }
    match parse_service_properties(&o.stdout) {
        Ok(rows) => {
            set.records = rows;
            if set.records.len() != c.selected_services.len() && set.status == Status::Ok {
                set.status = Status::Partial;
                set.limitations
                    .push("Selected service response count mismatch".into());
            }
        }
        Err(e) => {
            set.status = Status::Failed;
            set.limitations.push(e);
        }
    }
    set.duration_ms = start.elapsed().as_millis() as u64;
    set
}
pub fn collect(
    s: &dyn Sources,
    run: &dyn Runner,
    c: &Config,
    now: chrono::DateTime<chrono::Utc>,
    id: String,
    label: String,
    source: String,
) -> Snapshot {
    let mut sys = ResultSet::empty(Status::Ok, "os-release; proc/sys; proc/uptime", "");
    let mut r = Row::new();
    match text(s, "/etc/os-release", c) {
        Ok(t) => {
            for l in t.lines() {
                if let Some((k, v)) = l.split_once('=')
                    && ["ID", "VERSION_ID", "ID_LIKE"].contains(&k)
                {
                    r.insert(k.into(), v.trim_matches('"').to_string());
                }
            }
        }
        Err(e) => {
            sys.status = Status::Partial;
            sys.limitations.push(e.to_string());
        }
    }
    let distro = r.get("ID").cloned().unwrap_or_default();
    for (path, key) in [
        ("/proc/sys/kernel/osrelease", "kernel_release"),
        ("/proc/sys/kernel/version", "kernel_version"),
        ("/proc/uptime", "uptime"),
        ("/sys/hypervisor/type", "virtualization"),
    ] {
        match text(s, path, c) {
            Ok(t) => {
                r.insert(key.into(), t);
            }
            Err(e) => {
                if key != "virtualization" {
                    sys.status = Status::Partial;
                    sys.limitations.push(format!("{path}: {:?}", e.kind()));
                }
            }
        }
    }
    r.insert("architecture".into(), std::env::consts::ARCH.into());
    let boot_id = text(s, "/proc/sys/kernel/random/boot_id", c).ok();
    sys.records.push(r);
    let mut sets = BTreeMap::new();
    sets.insert("system".into(), sys);
    sets.insert("packages".into(), packages(run, &distro));
    for (kind, base) in [
        ("pci", "/sys/bus/pci/devices"),
        ("usb", "/sys/bus/usb/devices"),
        ("drm", "/sys/class/drm"),
        ("network", "/sys/class/net"),
        ("rfkill", "/sys/class/rfkill"),
        ("audio", "/sys/class/sound"),
    ] {
        sets.insert(kind.into(), devices(s, c, kind, base));
    }
    sets.insert("services".into(), services(run));
    sets.insert("selected_services".into(), selected_services(run, c));
    sets.insert("journal".into(), journal(run, c, now));
    let known = [
        text(s, "/proc/sys/kernel/hostname", c).ok(),
        std::env::var("USER").ok(),
        std::env::var("HOME").ok(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    for set in sets.values_mut() {
        for r in &mut set.records {
            for (k, v) in r.iter_mut() {
                if ["message", "kernel_version", "id"].contains(&k.as_str()) {
                    *v = privacy::redact(v, &known);
                }
            }
        }
        for l in &mut set.limitations {
            *l = privacy::redact(l, &known);
        }
    }
    Snapshot {
        schema_version: SCHEMA,
        application_version: env!("CARGO_PKG_VERSION").into(),
        id,
        started_at: now.to_rfc3339(),
        finished_at: chrono::Utc::now().to_rfc3339(),
        boot_id,
        source,
        label: privacy::redact(&label, &known),
        completeness: if sets.values().all(|s| s.status == Status::Ok) {
            Status::Ok
        } else {
            Status::Partial
        },
        privilege: if unsafe { libc::geteuid() } == 0 {
            "root"
        } else {
            "user"
        }
        .into(),
        collectors: sets,
    }
}
