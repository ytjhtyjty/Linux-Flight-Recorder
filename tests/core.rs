use linux_flight_recorder::{
    collect::{self, Sources},
    diff::{self, ChangeKind},
    model::{self, Config, ResultSet, Snapshot, Status, row},
    privacy, report,
    runner::{self, Output, Runner},
    storage::{self, Store},
};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn snapshot(n: u8) -> Snapshot {
    Snapshot {
        schema_version: 1,
        application_version: "0.1.0".into(),
        id: format!("{n:032x}"),
        started_at: "2026-10-29T12:00:00+00:00".into(),
        finished_at: "2026-10-29T12:00:01+00:00".into(),
        boot_id: Some("boot".into()),
        source: "manual".into(),
        label: "fixture".into(),
        completeness: Status::Ok,
        privilege: "user".into(),
        collectors: BTreeMap::new(),
    }
}
fn set(rows: Vec<model::Row>) -> ResultSet {
    ResultSet {
        status: Status::Ok,
        sources: vec!["fixture".into()],
        limitations: vec![],
        records: rows,
        duration_ms: 0,
    }
}
#[test]
fn package_fixtures() {
    for (adapter, file) in [
        ("dpkg", include_bytes!("fixtures/dpkg.tsv").as_slice()),
        ("rpm", include_bytes!("fixtures/rpm.tsv").as_slice()),
        ("pacman", include_bytes!("fixtures/pacman.txt").as_slice()),
    ] {
        let p = collect::parse_packages(adapter, file).unwrap();
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|r| r["adapter"] == adapter));
    }
}
#[test]
fn packages_corrupt_truncated_non_utf8() {
    for b in [b"bad\n".as_slice(), b"pkg\t1\tx86", b"\xff\n"] {
        assert!(collect::parse_packages("rpm", b).is_err());
    }
}
#[test]
fn multiple_installed_versions_retained() {
    assert_eq!(
        collect::parse_packages("rpm", b"a\t1\tx\na\t2\tx\n")
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn journal_unknown_and_binary_fields() {
    let r = collect::parse_journal(
        b"{\"MESSAGE\":[65,66],\"UNKNOWN\":\"secret\",\"_HOSTNAME\":\"machine\"}\n",
        4,
    )
    .unwrap();
    assert!(r[0]["message"].contains("omitted"));
    assert_eq!(r[0].len(), 1);
}
#[test]
fn journal_corrupt_limits() {
    assert!(collect::parse_journal(b"{}", 2).is_err());
    assert!(collect::parse_journal(b"{}\n", 2).is_err());
    assert!(collect::parse_journal(b"{\"MESSAGE\":\"ok\"}\n", 0).is_err());
    assert!(collect::parse_journal(b"\xff", 2).is_err());
}
#[test]
fn packages_added_removed_version_neutral() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert(
        "packages".into(),
        set(collect::parse_packages("rpm", b"a\t9\tx\nb\t1\tx\n").unwrap()),
    );
    b.collectors.insert(
        "packages".into(),
        set(collect::parse_packages("rpm", b"a\t10\tx\nc\t1\tx\n").unwrap()),
    );
    let d = diff::compare(&a, &b).unwrap();
    assert_eq!(
        d.changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Changed)
            .count(),
        1
    );
    assert_eq!(
        d.changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Added)
            .count(),
        1
    );
    assert_eq!(
        d.changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Removed)
            .count(),
        1
    );
    assert!(d.findings.is_empty());
}
fn pci(driver: bool) -> model::Row {
    let mut r = row(&[
        ("vendor", "0x8086"),
        ("device", "0x1234"),
        ("class", "0x028000"),
        ("location", "0000:00:01.0"),
    ]);
    if driver {
        r.insert("driver".into(), "iwlwifi".into());
    }
    r
}
#[test]
fn missing_driver_evidence_rule() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert("pci".into(), set(vec![pci(true)]));
    b.collectors.insert("pci".into(), set(vec![pci(false)]));
    let d = diff::compare(&a, &b).unwrap();
    assert_eq!(d.findings[0].rule_id, "LFR-DRIVER-001");
    assert_eq!(d.findings[0].evidence.len(), 2);
    assert!(!d.findings[0].alternatives.is_empty());
}
#[test]
fn source_unavailable_not_disappearance() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert("pci".into(), set(vec![pci(true)]));
    b.collectors.insert(
        "pci".into(),
        ResultSet::empty(Status::PermissionDenied, "sysfs", "denied"),
    );
    let d = diff::compare(&a, &b).unwrap();
    assert!(
        d.changes
            .iter()
            .all(|c| c.kind == ChangeKind::NotComparable)
    );
    assert!(d.findings.is_empty());
}
#[test]
fn device_absence_only_complete() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert("pci".into(), set(vec![pci(true)]));
    b.collectors.insert("pci".into(), set(vec![]));
    assert_eq!(
        diff::compare(&a, &b).unwrap().changes[0].kind,
        ChangeKind::Removed
    );
}
#[test]
fn ambiguous_duplicates_not_paired() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors
        .insert("pci".into(), set(vec![pci(true), pci(true)]));
    b.collectors.insert("pci".into(), set(vec![pci(false)]));
    let d = diff::compare(&a, &b).unwrap();
    assert_eq!(d.changes[0].kind, ChangeKind::NotComparable);
    assert!(d.findings.is_empty());
}
#[test]
fn pci_renumber_is_not_removal() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert("pci".into(), set(vec![pci(true)]));
    let mut r = pci(true);
    r.insert("location".into(), "0000:33:01.0".into());
    b.collectors.insert("pci".into(), set(vec![r]));
    assert_eq!(
        diff::compare(&a, &b).unwrap().changes[0].kind,
        ChangeKind::Unchanged
    );
}
#[test]
fn journal_never_new_when_unavailable() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert(
        "journal".into(),
        ResultSet::empty(Status::Unavailable, "j", "absent"),
    );
    b.collectors
        .insert("journal".into(), set(vec![row(&[("message", "error")])]));
    let d = diff::compare(&a, &b).unwrap();
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].kind, ChangeKind::NotComparable);
}
#[test]
fn failed_service_rule() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert("services".into(), set(vec![]));
    b.collectors.insert(
        "services".into(),
        set(vec![row(&[
            ("name", "audio.service"),
            ("active", "failed"),
        ])]),
    );
    assert_eq!(
        diff::compare(&a, &b).unwrap().findings[0].rule_id,
        "LFR-SERVICE-001"
    );
}
#[test]
fn future_snapshot_schema_rejected() {
    let a = snapshot(1);
    let mut b = snapshot(2);
    b.schema_version = 2;
    assert!(diff::compare(&a, &b).is_err());
}
#[test]
fn privacy_patterns_and_known_identity() {
    let t = privacy::redact(
        "hostalice alice 192.168.1.20 aa:bb:cc:dd:ee:ff /home/alice/secret Bearer abc token=qwerty me@example.com 2001:db8::1",
        &["hostalice".into(), "alice".into()],
    );
    for x in [
        "hostalice",
        "alice",
        "192.168",
        "aa:bb",
        "qwerty",
        "abc",
        "me@example",
        "db8",
    ] {
        assert!(!t.contains(x), "{t}");
    }
}
#[test]
fn escape_terminal_markdown_bidi() {
    let t = privacy::visible("\x1b[31m<page>#evil\n\u{202e}");
    assert!(!t.contains('\x1b'));
    assert!(!t.contains('<'));
    assert!(!t.contains('\n'));
    assert!(!t.contains('\u{202e}'));
}
#[test]
fn public_export_removes_linking_fields() {
    let mut s = snapshot(1);
    s.collectors.insert(
        "journal".into(),
        set(vec![row(&[
            ("message", "token=foo"),
            ("cursor", "abc"),
            ("boot_id", "boot"),
        ])]),
    );
    let p = privacy::public_snapshot(&s);
    assert!(p.boot_id.is_none());
    let r = &p.collectors["journal"].records[0];
    assert!(!r.contains_key("cursor"));
    assert!(!r.contains_key("boot_id"));
    assert!(!r["message"].contains("foo"));
}
fn store() -> (tempfile::TempDir, Store) {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let s = Store::open(d.path()).unwrap();
    (d, s)
}
#[test]
fn storage_roundtrip_integrity_and_migration() {
    let (_d, mut st) = store();
    st.save(&snapshot(1), &Config::default()).unwrap();
    assert_eq!(st.load(&format!("{:032x}", 1)).unwrap().id, snapshot(1).id);
    assert_eq!(
        st.connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    st.connection
        .execute("UPDATE snapshots SET sha256='bad'", [])
        .unwrap();
    assert!(st.load(&snapshot(1).id).is_err());
}
#[test]
fn future_database_schema_rejected() {
    let (d, st) = store();
    st.connection
        .pragma_update(None, "user_version", 2)
        .unwrap();
    drop(st);
    assert!(Store::open(d.path()).is_err());
}
#[test]
fn concurrent_store_lock() {
    let (d, _st) = store();
    assert!(Store::open(d.path()).unwrap_err_string().contains("busy"));
}
trait ErrorString<T> {
    fn unwrap_err_string(self) -> String;
}
impl<T> ErrorString<T> for anyhow::Result<T> {
    fn unwrap_err_string(self) -> String {
        match self {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        }
    }
}
#[test]
fn unsafe_store_permissions_and_symlink() {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Store::open(d.path()).is_err());
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    symlink(d.path(), d.path().join("alias")).unwrap();
    assert!(Store::open(&d.path().join("alias")).is_err());
    symlink("/tmp/not-lfr", d.path().join("index.sqlite3")).unwrap();
    assert!(Store::open(d.path()).is_err());
}
#[test]
fn traversal_and_invalid_id_rejected() {
    let (_d, mut st) = store();
    assert!(st.load("../../etc/shadow").is_err());
    assert!(st.delete("../", false).is_err());
}
#[test]
fn retention_pin_and_dry_run() {
    let (_d, mut st) = store();
    for n in 1..=3 {
        st.save(&snapshot(n), &Config::default()).unwrap();
    }
    st.pin(&snapshot(1).id).unwrap();
    let c = Config {
        max_snapshots: 1,
        ..Default::default()
    };
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let p = st.gc(&c, now, true).unwrap();
    assert_eq!(p.delete.len(), 2);
    assert_eq!(st.list().unwrap().len(), 3);
    st.gc(&c, now, false).unwrap();
    assert_eq!(st.list().unwrap().len(), 1);
    assert!(st.delete(&snapshot(1).id, false).is_err());
}
#[test]
fn pins_over_quota_reported() {
    let (_d, mut st) = store();
    for n in 1..=2 {
        st.save(&snapshot(n), &Config::default()).unwrap();
        st.pin(&snapshot(n).id).unwrap();
    }
    let c = Config {
        max_snapshots: 1,
        ..Default::default()
    };
    let p = st.gc(&c, chrono::Utc::now(), true).unwrap();
    assert!(p.pinned_limit_exceeded);
    assert!(p.delete.is_empty());
}
#[test]
fn failed_transaction_rolls_back() {
    let (_d, mut st) = store();
    let tx = st.connection.transaction().unwrap();
    tx.execute(
        "INSERT INTO snapshots VALUES('x','t','l','ok',0,1,'h',x'00')",
        [],
    )
    .unwrap();
    drop(tx);
    assert!(st.list().unwrap().is_empty());
}
#[test]
fn sqlite_disk_full_simulated() {
    let (_d, mut st) = store();
    let pages: i64 = st
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    st.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let mut s = snapshot(1);
    s.label = "x".repeat(100000);
    let e = st.save(&s, &Config::default()).unwrap_err().to_string();
    assert!(e.contains("full"), "{e}");
    assert!(st.list().unwrap().is_empty());
}
#[test]
fn quota_failure_is_atomic() {
    let (_d, mut st) = store();
    let c = Config {
        max_store_bytes: 1,
        ..Default::default()
    };
    assert!(st.save(&snapshot(1), &c).is_err());
    assert!(st.list().unwrap().is_empty());
}
#[test]
fn output_no_overwrite_or_symlink_and_failure_cleanup() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("out");
    storage::write_bytes(&p, b"first").unwrap();
    assert!(storage::write_bytes(&p, b"second").is_err());
    assert_eq!(fs::read(&p).unwrap(), b"first");
    let link = d.path().join("link");
    symlink(&p, &link).unwrap();
    assert!(storage::write_bytes(&link, b"bad").is_err());
    let fail = d.path().join("fail");
    assert!(storage::write_output(&fail, |_| anyhow::bail!("simulated ENOSPC")).is_err());
    assert!(!fail.exists());
}
#[test]
fn actual_enospc_device() {
    use std::io::Write;
    let e = fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap()
        .write_all(b"test")
        .unwrap_err();
    assert_eq!(e.raw_os_error(), Some(libc::ENOSPC));
}
#[test]
fn export_manifest_and_reproducible_members() {
    let d = tempfile::tempdir().unwrap();
    let a = d.path().join("a.tar.gz");
    let b = d.path().join("b.tar.gz");
    report::export(&snapshot(1), &a).unwrap();
    report::export(&snapshot(1), &b).unwrap();
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    let gz = flate2::read::GzDecoder::new(fs::File::open(a).unwrap());
    let mut ar = tar::Archive::new(gz);
    let mut names = vec![];
    let mut data = BTreeMap::new();
    use std::io::Read;
    for e in ar.entries().unwrap() {
        let mut e = e.unwrap();
        let name = e.path().unwrap().display().to_string();
        names.push(name.clone());
        let mut b = vec![];
        e.read_to_end(&mut b).unwrap();
        data.insert(name, b);
    }
    assert_eq!(names, report::MEMBERS);
    let m: serde_json::Value = serde_json::from_slice(&data["manifest.json"]).unwrap();
    assert_eq!(
        m["files"][0]["sha256"],
        storage::hash(&data["snapshot.json"])
    );
}
#[test]
fn command_timeout_and_process_group() {
    let start = Instant::now();
    let o = runner::bounded_command(
        Path::new("/usr/bin/sleep"),
        &["10"],
        Duration::from_millis(70),
        1024,
        &AtomicBool::new(false),
    );
    assert_eq!(o.status, Status::Timeout);
    assert!(start.elapsed() < Duration::from_secs(2));
}
#[test]
fn command_output_limit() {
    let o = runner::bounded_command(
        Path::new("/usr/bin/yes"),
        &[],
        Duration::from_secs(2),
        1024,
        &AtomicBool::new(false),
    );
    assert_eq!(o.status, Status::Partial);
    assert!(o.stdout.len() <= 1024);
}
#[test]
fn command_cancel() {
    let o = runner::bounded_command(
        Path::new("/usr/bin/sleep"),
        &["10"],
        Duration::from_secs(2),
        1024,
        &AtomicBool::new(true),
    );
    assert_eq!(o.status, Status::Failed);
    assert!(o.diagnostic.contains("interrupted"));
}
#[test]
fn command_permission_denied() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("not-executable");
    fs::write(&p, b"not a program").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    let o = runner::bounded_command(
        &p,
        &[],
        Duration::from_secs(1),
        100,
        &AtomicBool::new(false),
    );
    assert_eq!(o.status, Status::PermissionDenied);
}
#[test]
fn trusted_search_rejects_path_injection() {
    assert!(runner::executable("../../bin/sh").is_none());
    assert!(runner::executable("a;id").is_none());
}
struct FakeSources {
    denied: bool,
    vanish: bool,
}
impl Sources for FakeSources {
    fn read(&self, p: &str, _: usize) -> io::Result<Vec<u8>> {
        if self.denied {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if p == "/etc/os-release" {
            return Ok(b"ID=unknown\nUNKNOWN=ignored\n".to_vec());
        }
        if self.vanish && p.contains("pci") {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(b"0".to_vec())
    }
    fn entries(&self, p: &str, _: usize) -> io::Result<Vec<String>> {
        if self.denied {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(if p.contains("pci") {
            vec!["0000:00:00.0".into()]
        } else {
            vec![]
        })
    }
    fn link(&self, _: &str) -> io::Result<String> {
        Err(io::ErrorKind::NotFound.into())
    }
}
struct MissingRunner;
impl Runner for MissingRunner {
    fn run(&self, _: &str, _: &[&str]) -> Output {
        Output {
            status: Status::Unavailable,
            stdout: vec![],
            diagnostic: "missing fixture tool".into(),
        }
    }
}
#[test]
fn unknown_distro_partial_no_systemd() {
    let s = collect::collect(
        &FakeSources {
            denied: false,
            vanish: false,
        },
        &MissingRunner,
        &Config::default(),
        chrono::Utc::now(),
        snapshot(1).id,
        "label".into(),
        "manual".into(),
    );
    assert_eq!(s.collectors["packages"].status, Status::Unavailable);
    assert_eq!(s.collectors["services"].status, Status::Unavailable);
    assert_eq!(s.completeness, Status::Partial);
}
#[test]
fn disappeared_device_partial() {
    let s = collect::collect(
        &FakeSources {
            denied: false,
            vanish: true,
        },
        &MissingRunner,
        &Config::default(),
        chrono::Utc::now(),
        snapshot(1).id,
        "".into(),
        "manual".into(),
    );
    assert_eq!(s.collectors["pci"].status, Status::Partial);
}
#[test]
fn denied_sources_not_empty_success() {
    let s = collect::collect(
        &FakeSources {
            denied: true,
            vanish: false,
        },
        &MissingRunner,
        &Config::default(),
        chrono::Utc::now(),
        snapshot(1).id,
        "".into(),
        "manual".into(),
    );
    assert_eq!(s.collectors["pci"].status, Status::PermissionDenied);
}
#[test]
fn config_unknown_invalid() {
    assert!(serde_json::from_str::<Config>("{\"surprise\":1}").is_err());
    assert!(
        Config {
            command_timeout_ms: 0,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(Config::default().validate().is_ok());
}
#[test]
fn deterministic_order() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert(
        "packages".into(),
        set(collect::parse_packages("rpm", b"z\t1\tx\na\t1\tx\n").unwrap()),
    );
    b.collectors.insert(
        "packages".into(),
        set(collect::parse_packages("rpm", b"a\t2\tx\nz\t2\tx\n").unwrap()),
    );
    let d = diff::compare(&a, &b).unwrap();
    a.collectors.get_mut("packages").unwrap().records.reverse();
    b.collectors.get_mut("packages").unwrap().records.reverse();
    let d2 = diff::compare(&a, &b).unwrap();
    assert_eq!(
        d.changes.iter().map(|c| &c.identity).collect::<Vec<_>>(),
        d2.changes.iter().map(|c| &c.identity).collect::<Vec<_>>()
    );
}
#[test]
fn golden_json_markdown() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    b.boot_id = Some("other-boot".into());
    a.collectors.insert(
        "system".into(),
        set(vec![row(&[("ID", "debian"), ("kernel_release", "6.1")])]),
    );
    b.collectors.insert(
        "system".into(),
        set(vec![row(&[("ID", "debian"), ("kernel_release", "6.2")])]),
    );
    a.collectors.insert("pci".into(), set(vec![pci(true)]));
    b.collectors.insert("pci".into(), set(vec![pci(false)]));
    a.collectors.insert(
        "packages".into(),
        set(collect::parse_packages("rpm", b"kernel\t6.1\tx\nfirmware\t1\tx\n").unwrap()),
    );
    b.collectors.insert(
        "packages".into(),
        set(collect::parse_packages("rpm", b"kernel\t6.2\tx\nfirmware\t2\tx\n").unwrap()),
    );
    a.collectors.insert("services".into(), set(vec![]));
    b.collectors.insert(
        "services".into(),
        set(vec![row(&[("name", "wifi.service"), ("active", "failed")])]),
    );
    a.collectors.insert(
        "journal".into(),
        ResultSet::empty(Status::Unavailable, "fixture journal", "access unavailable"),
    );
    let mut j = set(vec![row(&[(
        "message",
        "driver init failed <untrusted> token=private 192.168.1.1",
    )])]);
    j.status = Status::Partial;
    b.collectors.insert("journal".into(), j);
    let d = diff::compare(&a, &b).unwrap();
    let json = format!("{}\n", serde_json::to_string_pretty(&d).unwrap());
    let md = report::markdown(&a, &b, &d, Some("wifi"));
    if std::env::var_os("LFR_UPDATE_GOLDEN").is_some() {
        fs::write("tests/fixtures/comparison.json", &json).unwrap();
        fs::write("tests/fixtures/report.md", &md).unwrap();
    }
    assert_eq!(
        json,
        fs::read_to_string("tests/fixtures/comparison.json").unwrap()
    );
    assert_eq!(md, fs::read_to_string("tests/fixtures/report.md").unwrap());
}
proptest::proptest! { # [test] fn package_parser_never_panics (b in proptest :: collection :: vec (proptest :: num :: u8 :: ANY , 0 .. 4096)) { let _ = collect :: parse_packages ("rpm" ,& b) ; let _ = collect :: parse_packages ("dpkg" ,& b) ; let _ = collect :: parse_journal (& b , 200) ; } # [test] fn visible_has_no_controls (s in ".{0,1000}") { let v = privacy :: visible (& s) ; proptest :: prop_assert ! (! v . chars () . any (char :: is_control)) ; } }
#[test]
fn incomplete_driver_source_does_not_emit_loss_rule() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert("pci".into(), set(vec![pci(true)]));
    let mut after = set(vec![pci(false)]);
    after.status = Status::Partial;
    b.collectors.insert("pci".into(), after);
    assert!(diff::compare(&a, &b).unwrap().findings.is_empty());
}
#[test]
fn crash_writer_child() {
    if let Some(path) = std::env::var_os("LFR_CRASH_TEST_DB") {
        let mut c = rusqlite::Connection::open(path).unwrap();
        let tx = c.transaction().unwrap();
        tx.execute(
            "INSERT INTO snapshots VALUES('crash','t','l','ok',0,1,'h',x'00')",
            [],
        )
        .unwrap();
        std::process::exit(77);
    }
}
#[test]
fn interrupted_process_sqlite_recovery() {
    let (d, mut st) = store();
    st.save(&snapshot(1), &Config::default()).unwrap();
    drop(st);
    let code = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_writer_child"])
        .env("LFR_CRASH_TEST_DB", d.path().join("index.sqlite3"))
        .output()
        .unwrap()
        .status
        .code();
    assert_eq!(code, Some(77));
    let st = Store::open(d.path()).unwrap();
    assert_eq!(st.list().unwrap().len(), 1);
    assert_eq!(st.load(&snapshot(1).id).unwrap().id, snapshot(1).id);
}
#[test]
fn bounded_sources_entry_and_byte_limits() {
    use collect::LinuxSources;
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("one"), b"12345").unwrap();
    fs::write(d.path().join("two"), b"z").unwrap();
    let s = LinuxSources {
        root: d.path().to_owned(),
    };
    assert!(s.read("/one", 3).is_err());
    assert!(s.entries("/", 1).is_err());
    assert_eq!(s.read("/one", 5).unwrap(), b"12345");
}
#[test]
fn ipv6_redacted_without_destroying_timestamps_or_pci() {
    let t = "2026-10-29T21:57:44.123+00:00 0000:00:1f.0";
    assert_eq!(privacy::redact(t, &[]), t);
    for address in [
        "::1",
        "::ffff:192.168.1.1",
        "fe80::abcd%eth0",
        "2001:db8::1",
    ] {
        assert!(!privacy::redact(address, &[]).contains(address));
    }
}
#[test]
fn install_only_package_version_sets() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert(
        "packages".into(),
        set(collect::parse_packages("rpm", b"kernel\t6.1\tx\nkernel\t6.2\tx\n").unwrap()),
    );
    b.collectors.insert(
        "packages".into(),
        set(collect::parse_packages("rpm", b"kernel\t6.2\tx\n").unwrap()),
    );
    let d = diff::compare(&a, &b).unwrap();
    assert_eq!(d.changes.len(), 2);
    assert_eq!(
        d.changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Removed)
            .count(),
        1
    );
    assert_eq!(
        d.changes
            .iter()
            .filter(|c| c.kind == ChangeKind::Unchanged)
            .count(),
        1
    );
}
#[test]
fn network_renumber_no_false_disappearance() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert(
        "network".into(),
        set(vec![row(&[
            ("location", "eth0"),
            ("type", "1"),
            ("operstate", "up"),
        ])]),
    );
    b.collectors.insert(
        "network".into(),
        set(vec![row(&[
            ("location", "enp3s0"),
            ("type", "1"),
            ("operstate", "up"),
        ])]),
    );
    assert_eq!(
        diff::compare(&a, &b).unwrap().changes[0].kind,
        ChangeKind::Unchanged
    );
}
#[test]
fn selected_service_properties_unknown_and_malformed() {
    let r = collect :: parse_service_properties (b"Id=NetworkManager.service\nLoadState=loaded\nActiveState=active\nSubState=running\nUnknown=ignored\n\n") . unwrap () ;
    assert_eq!(r[0]["active"], "active");
    assert_eq!(r[0].len(), 4);
    assert!(collect::parse_service_properties(b"Id=x\n").is_err());
    assert!(collect::parse_service_properties(b"Id=x").is_err());
    assert!(collect::parse_service_properties(b"\xff\n").is_err());
    assert!(
        Config {
            selected_services: vec!["--host=remote.service".into()],
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}
#[test]
fn unversioned_nonempty_database_rejected() {
    let d = tempfile::tempdir().unwrap();
    let c = rusqlite::Connection::open(d.path().join("index.sqlite3")).unwrap();
    c.execute("CREATE TABLE unexpected(x)", []).unwrap();
    drop(c);
    fs::set_permissions(
        d.path().join("index.sqlite3"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(Store::open(d.path()).is_err());
}
#[test]
fn blob_length_checked_before_materializing() {
    let (_d, mut st) = store();
    st.save(&snapshot(1), &Config::default()).unwrap();
    st.connection
        .execute("UPDATE snapshots SET bytes=1", [])
        .unwrap();
    assert!(st.load(&snapshot(1).id).is_err());
}
#[test]
fn command_environment_is_minimal() {
    let path = runner::executable("env").expect("env installed on Linux CI");
    let o = runner::bounded_command(
        &path,
        &[],
        Duration::from_secs(1),
        4096,
        &AtomicBool::new(false),
    );
    assert_eq!(o.status, Status::Ok);
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(text.contains("LC_ALL=C"));
    assert!(!text.contains("HOME="));
    assert!(!text.contains("USER="));
    assert!(!text.contains("TOKEN="));
}
#[test]
fn changed_selection_scope_not_service_removal() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    let mut first = set(vec![row(&[("name", "a.service"), ("active", "active")])]);
    first.sources = vec!["units=a.service".into()];
    let mut second = set(vec![]);
    second.sources = vec!["units=b.service".into()];
    a.collectors.insert("selected_services".into(), first);
    b.collectors.insert("selected_services".into(), second);
    assert!(
        diff::compare(&a, &b)
            .unwrap()
            .changes
            .iter()
            .all(|c| c.kind == ChangeKind::NotComparable)
    );
}
#[test]
fn unreadable_kernel_is_not_a_kernel_change() {
    let mut a = snapshot(1);
    let mut b = snapshot(2);
    a.collectors.insert(
        "system".into(),
        set(vec![row(&[("ID", "debian"), ("kernel_release", "6.1")])]),
    );
    let mut sys = set(vec![row(&[("ID", "debian")])]);
    sys.status = Status::Partial;
    b.collectors.insert("system".into(), sys);
    a.collectors.insert("pci".into(), set(vec![pci(true)]));
    b.collectors.insert("pci".into(), set(vec![pci(false)]));
    let d = diff::compare(&a, &b).unwrap();
    assert_eq!(d.findings.len(), 1);
    assert!(!d.findings[0].hypothesis.contains("kernel change"));
    assert_eq!(d.findings[0].evidence.len(), 2);
}
#[test]
fn journalctl_access_denial_is_not_generic_failure() {
    struct JournalExit(Status, &'static str);
    impl Runner for JournalExit {
        fn run(&self, program: &str, _: &[&str]) -> Output {
            if program == "journalctl" {
                Output {
                    status: self.0.clone(),
                    stdout: vec![],
                    diagnostic: self.1.into(),
                }
            } else {
                MissingRunner.run(program, &[])
            }
        }
    }
    for (input, diagnostic, expected) in [
        (
            Status::Failed,
            "exit exit status: 1: No journal files were opened due to insufficient permissions.",
            Status::PermissionDenied,
        ),
        (Status::Failed, "unrelated journal failure", Status::Failed),
        (
            Status::Timeout,
            "No journal files were opened due to insufficient permissions.",
            Status::Timeout,
        ),
    ] {
        let s = collect::collect(
            &FakeSources {
                denied: false,
                vanish: false,
            },
            &JournalExit(input, diagnostic),
            &Config::default(),
            chrono::Utc::now(),
            snapshot(1).id,
            "".into(),
            "manual".into(),
        );
        assert_eq!(s.collectors["journal"].status, expected);
        assert!(s.collectors["journal"].records.is_empty());
        assert_eq!(s.completeness, Status::Partial);
    }
}
