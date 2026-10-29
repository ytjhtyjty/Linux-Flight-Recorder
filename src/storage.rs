use crate::model::{Config, Snapshot, valid_id};
use anyhow::{Context, ensure};
use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};
pub const MAX_SNAPSHOT: usize = 32 * 1024 * 1024;
pub fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
pub fn private_dir(path: &Path) -> anyhow::Result<()> {
    ensure!(
        path.is_absolute(),
        "store/output parent path must be absolute"
    );
    let uid = unsafe { libc::geteuid() };
    let mut current = PathBuf::from("/");
    for component in path.components().skip(1) {
        ensure!(
            matches!(component, std::path::Component::Normal(_)),
            "non-normal path component"
        );
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(m) => ensure!(
                m.is_dir() && !m.file_type().is_symlink(),
                "symlink/non-directory path component: {}",
                current.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new().mode(0o700).create(&current)?;
            }
            Err(e) => return Err(e.into()),
        }
        let m = fs::symlink_metadata(&current)?;
        ensure!(
            m.uid() == uid || m.uid() == 0,
            "untrusted directory owner: {}",
            current.display()
        );
        ensure!(
            m.mode() & 0o022 == 0 || m.mode() & 0o1000 != 0,
            "writable non-sticky directory: {}",
            current.display()
        );
    }
    let m = fs::symlink_metadata(path)?;
    ensure!(
        m.uid() == uid && m.mode() & 0o077 == 0,
        "store must be owned by current uid with mode 0700: {}",
        path.display()
    );
    Ok(())
}
fn private_file(path: &Path) -> anyhow::Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file()
            && m.uid() == unsafe { libc::geteuid() }
            && m.mode() & 0o077 == 0
            && m.nlink() == 1,
        "unsafe store file {}",
        path.display()
    );
    Ok(f)
}
pub struct Store {
    pub connection: Connection,
    _lock: File,
}
#[derive(Serialize, Debug)]
pub struct Entry {
    pub id: String,
    pub started_at: String,
    pub label: String,
    pub completeness: String,
    pub pinned: bool,
    pub bytes: u64,
}
#[derive(Serialize, Debug)]
pub struct GcPlan {
    pub delete: Vec<String>,
    pub retained_bytes: u64,
    pub pinned_limit_exceeded: bool,
}
impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        private_dir(path)?;
        let lock = private_file(&path.join("lock"))?;
        ensure!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "store busy: another lfr operation holds the lock"
        );
        let db = path.join("index.sqlite3");
        let _file = private_file(&db)?;
        let mut connection = Connection::open(&db)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(
            (0..=1).contains(&version),
            "database schema {version} is unsupported (supported: 0..1)"
        );
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        if version == 0 {
            let tables : i64 = connection . query_row ("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'" , [] ,| r | r . get (0)) ?;
            ensure!(
                tables == 0,
                "unversioned non-empty database; refusing to guess migration"
            );
            let tx = connection.transaction()?;
            tx . execute_batch ("CREATE TABLE IF NOT EXISTS snapshots (id TEXT PRIMARY KEY, started_at TEXT NOT NULL, label TEXT NOT NULL, completeness TEXT NOT NULL, pinned INTEGER NOT NULL DEFAULT 0, bytes INTEGER NOT NULL, sha256 TEXT NOT NULL, json BLOB NOT NULL); PRAGMA user_version=1;") ?;
            tx.commit()?;
        }
        Ok(Self {
            connection,
            _lock: lock,
        })
    }
    pub fn save(&mut self, s: &Snapshot, c: &Config) -> anyhow::Result<()> {
        s.validate()?;
        let b = serde_json::to_vec(s)?;
        ensure!(b.len() <= MAX_SNAPSHOT, "snapshot size limit exceeded");
        let tx = self.connection.transaction()?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))?;
        ensure!(count < 1000, "hard snapshot count budget exceeded; run gc");
        let used: i64 = tx.query_row("SELECT COALESCE(SUM(bytes),0) FROM snapshots", [], |r| {
            r.get(0)
        })?;
        ensure!(
            used >= 0 && (used as u64).saturating_add(b.len() as u64) <= c.max_store_bytes,
            "store quota exceeded; run gc (pinned snapshots are never removed automatically)"
        );
        tx . execute ("INSERT INTO snapshots(id,started_at,label,completeness,bytes,sha256,json) VALUES(?1,?2,?3,?4,?5,?6,?7)" , params ! [s . id , s . started_at , s . label , format ! ("{:?}" , s . completeness) . to_lowercase () , b . len () as i64 , hash (& b) , b]) ?;
        tx.commit()?;
        Ok(())
    }
    pub fn load(&self, id: &str) -> anyhow::Result<Snapshot> {
        ensure!(valid_id(id), "invalid snapshot id");
        let (bytes, actual): (i64, i64) = self
            .connection
            .query_row(
                "SELECT bytes,length(json) FROM snapshots WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .context("snapshot not found")?;
        ensure!(
            (0..=MAX_SNAPSHOT as i64).contains(&bytes) && actual == bytes,
            "snapshot exceeds read budget"
        );
        let (b, h): (Vec<u8>, String) = self.connection.query_row(
            "SELECT json,sha256 FROM snapshots WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        ensure!(
            b.len() <= MAX_SNAPSHOT && b.len() as i64 == bytes && hash(&b) == h,
            "snapshot integrity check failed"
        );
        let s: Snapshot = serde_json::from_slice(&b)?;
        s.validate()?;
        ensure!(s.id == id, "snapshot identity mismatch");
        Ok(s)
    }
    pub fn list(&self) -> anyhow::Result<Vec<Entry>> {
        let mut st = self . connection . prepare ("SELECT id,started_at,label,completeness,pinned,bytes FROM snapshots ORDER BY started_at,id") ?;
        Ok(st
            .query_map([], |r| {
                Ok(Entry {
                    id: r.get(0)?,
                    started_at: r.get(1)?,
                    label: r.get(2)?,
                    completeness: r.get(3)?,
                    pinned: r.get(4)?,
                    bytes: r.get::<_, i64>(5)?.max(0) as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn pin(&self, id: &str) -> anyhow::Result<()> {
        ensure!(valid_id(id), "invalid id");
        ensure!(
            self.connection
                .execute("UPDATE snapshots SET pinned=1 WHERE id=?1", [id])?
                == 1,
            "snapshot not found"
        );
        Ok(())
    }
    pub fn delete(&mut self, id: &str, allow_pin: bool) -> anyhow::Result<()> {
        ensure!(valid_id(id), "invalid id");
        let tx = self.connection.transaction()?;
        ensure!(
            tx.execute(
                "DELETE FROM snapshots WHERE id=?1 AND (pinned=0 OR ?2=1)",
                params![id, allow_pin]
            )? == 1,
            "snapshot not found or pinned (explicit --allow-pinned required)"
        );
        tx.commit()?;
        Ok(())
    }
    pub fn gc(
        &mut self,
        c: &Config,
        now: chrono::DateTime<chrono::Utc>,
        dry: bool,
    ) -> anyhow::Result<GcPlan> {
        let entries = self.list()?;
        let mut bytes = entries.iter().map(|s| s.bytes).sum::<u64>();
        let mut count = entries.len();
        let mut delete = vec![];
        for s in &entries {
            let old = chrono::DateTime::parse_from_rfc3339(&s.started_at)
                .map(|t| (now - t.with_timezone(&chrono::Utc)).num_days() > c.max_age_days as i64)
                .unwrap_or(false);
            if !s.pinned && (old || count > c.max_snapshots || bytes > c.max_store_bytes) {
                delete.push(s.id.clone());
                bytes -= s.bytes;
                count -= 1;
            }
        }
        if !dry {
            let tx = self.connection.transaction()?;
            for id in &delete {
                tx.execute("DELETE FROM snapshots WHERE id=?1 AND pinned=0", [id])?;
            }
            tx.commit()?;
            self.connection.execute_batch("VACUUM")?;
        }
        Ok(GcPlan {
            delete,
            retained_bytes: bytes,
            pinned_limit_exceeded: bytes > c.max_store_bytes || count > c.max_snapshots,
        })
    }
}
pub fn write_output(
    path: &Path,
    write: impl FnOnce(&mut File) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute.parent().context("output parent missing")?;
    let mut component_path = PathBuf::from("/");
    for c in parent.components().skip(1) {
        ensure!(
            matches!(c, std::path::Component::Normal(_)),
            "non-normal output path"
        );
        component_path.push(c);
        let m = fs::symlink_metadata(&component_path)?;
        ensure!(
            m.is_dir() && !m.file_type().is_symlink(),
            "symlink output parent not permitted; use its physical path"
        );
    }
    let mut random = [0_u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let token = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let tmp = parent.join(format!(".lfr-{token}.tmp"));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(tmp.clone());
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&tmp)?;
    write(&mut f)?;
    f.sync_all()?;
    let from = CString::new(tmp.as_os_str().as_bytes())?;
    let to = CString::new(absolute.as_os_str().as_bytes())?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error())
            .context("output must not exist; atomic rename failed");
    }
    File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn write_bytes(path: &Path, b: &[u8]) -> anyhow::Result<()> {
    write_output(path, |f| {
        f.write_all(b)?;
        Ok(())
    })
}
pub fn read_config(path: Option<&Path>) -> anyhow::Result<Config> {
    let c = if let Some(p) = path {
        let f = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(p)?;
        ensure!(f.metadata()?.len() <= 65536, "config too large");
        let mut b = vec![];
        f.take(65537).read_to_end(&mut b)?;
        serde_json::from_slice(&b)?
    } else {
        Config::default()
    };
    c.validate()?;
    Ok(c)
}
