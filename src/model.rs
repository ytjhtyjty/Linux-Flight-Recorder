use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const SCHEMA: u32 = 1;
pub type Row = BTreeMap<String, String>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Partial,
    Unavailable,
    PermissionDenied,
    Timeout,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResultSet {
    pub status: Status,
    pub sources: Vec<String>,
    pub limitations: Vec<String>,
    pub records: Vec<Row>,
    pub duration_ms: u64,
}
impl ResultSet {
    pub fn empty(status: Status, source: &str, reason: &str) -> Self {
        Self {
            status,
            sources: vec![source.into()],
            limitations: if reason.is_empty() {
                vec![]
            } else {
                vec![reason.into()]
            },
            records: vec![],
            duration_ms: 0,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub application_version: String,
    pub id: String,
    pub started_at: String,
    pub finished_at: String,
    pub boot_id: Option<String>,
    pub source: String,
    pub label: String,
    pub completeness: Status,
    pub privilege: String,
    pub collectors: BTreeMap<String, ResultSet>,
}
impl Snapshot {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == SCHEMA,
            "unsupported snapshot schema {}",
            self.schema_version
        );
        anyhow::ensure!(valid_id(&self.id), "invalid snapshot id");
        Ok(())
    }
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub command_timeout_ms: u64,
    pub command_output_bytes: usize,
    pub source_bytes: usize,
    pub source_entries: usize,
    pub source_timeout_ms: u64,
    pub journal_records: usize,
    pub journal_window_seconds: u64,
    pub selected_services: Vec<String>,
    pub max_snapshots: usize,
    pub max_age_days: u64,
    pub max_store_bytes: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            command_timeout_ms: 5000,
            command_output_bytes: 4 * 1024 * 1024,
            source_bytes: 16384,
            source_entries: 4096,
            source_timeout_ms: 3000,
            journal_records: 200,
            journal_window_seconds: 900,
            selected_services: vec![
                "NetworkManager.service".into(),
                "systemd-networkd.service".into(),
                "bluetooth.service".into(),
            ],
            max_snapshots: 40,
            max_age_days: 30,
            max_store_bytes: 256 * 1024 * 1024,
        }
    }
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (10..=30000).contains(&self.command_timeout_ms),
            "command_timeout_ms outside 10..30000"
        );
        anyhow::ensure!(
            (1024..=8 * 1024 * 1024).contains(&self.command_output_bytes),
            "invalid command_output_bytes"
        );
        anyhow::ensure!(
            (128..=65536).contains(&self.source_bytes),
            "invalid source_bytes"
        );
        anyhow::ensure!(
            (1..=16384).contains(&self.source_entries),
            "invalid source_entries"
        );
        anyhow::ensure!(
            (10..=10000).contains(&self.source_timeout_ms),
            "invalid source_timeout_ms"
        );
        anyhow::ensure!(
            (1..=1000).contains(&self.journal_records),
            "invalid journal_records"
        );
        anyhow::ensure!(
            (60..=3600).contains(&self.journal_window_seconds),
            "invalid journal window"
        );
        anyhow::ensure!(
            self.selected_services.len() <= 16
                && self.selected_services.iter().all(|s| !s.starts_with('-')
                    && s.ends_with(".service")
                    && s.len() <= 128
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"@_.-".contains(&b))),
            "invalid selected_services (maximum 16 safe service names)"
        );
        anyhow::ensure!(
            (1..=1000).contains(&self.max_snapshots),
            "invalid max_snapshots"
        );
        anyhow::ensure!(
            (1..=3650).contains(&self.max_age_days),
            "invalid max_age_days"
        );
        anyhow::ensure!(
            (1024 * 1024..=4 * 1024 * 1024 * 1024_u64).contains(&self.max_store_bytes),
            "invalid max_store_bytes"
        );
        Ok(())
    }
}
pub fn row(pairs: &[(&str, &str)]) -> Row {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}
