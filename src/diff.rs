use crate::model::{Row, Snapshot, Status};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
    Unchanged,
    NotComparable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub collector: String,
    pub identity: String,
    pub kind: ChangeKind,
    pub before: Option<Row>,
    pub after: Option<Row>,
    pub evidence: Vec<String>,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub category: String,
    pub severity: String,
    pub confidence_basis: String,
    pub observation: String,
    pub hypothesis: String,
    pub evidence: Vec<String>,
    pub alternatives: Vec<String>,
    pub limitations: Vec<String>,
    pub safe_checks: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Comparison {
    pub schema_version: u32,
    pub before_id: String,
    pub after_id: String,
    pub limitations: Vec<String>,
    pub changes: Vec<Change>,
    pub findings: Vec<Finding>,
}
fn val<'a>(r: &'a Row, k: &str) -> &'a str {
    r.get(k).map(String::as_str).unwrap_or("")
}
fn identity(kind: &str, r: &Row) -> String {
    let fields: &[&str] = match kind {
        "packages" => &["adapter", "name", "architecture"],
        "pci" => &[
            "vendor",
            "device",
            "subsystem_vendor",
            "subsystem_device",
            "class",
        ],
        "usb" => {
            if r.contains_key("idVendor") {
                &["idVendor", "idProduct", "bDeviceClass", "interface"]
            } else {
                &["location"]
            }
        }
        "network" | "audio" => {
            if r.contains_key("hardware_vendor") {
                &[
                    "hardware_vendor",
                    "hardware_device",
                    "hardware_subsystem_vendor",
                    "hardware_subsystem_device",
                ]
            } else {
                if kind == "network" {
                    &["type"]
                } else {
                    &["id"]
                }
            }
        }
        "drm" => {
            if r.contains_key("hardware_vendor") {
                &["hardware_vendor", "hardware_device", "connector"]
            } else {
                &["connector"]
            }
        }
        "rfkill" => &["type"],
        "services" | "selected_services" => &["name"],
        "system" => &["ID"],
        _ => &["location"],
    };
    fields
        .iter()
        .map(|f| format!("{f}={}", val(r, f)))
        .collect::<Vec<_>>()
        .join(";")
}
fn index<'a>(kind: &str, rows: &'a [Row]) -> BTreeMap<String, Vec<(usize, &'a Row)>> {
    let mut m: BTreeMap<String, Vec<(usize, &Row)>> = BTreeMap::new();
    for (i, r) in rows.iter().enumerate() {
        m.entry(identity(kind, r)).or_default().push((i, r));
    }
    m
}
fn meaningful(kind: &str, r: &Row) -> Row {
    let mut r = r.clone();
    if ["pci", "usb", "drm", "network", "audio"].contains(&kind) {
        r.remove("location");
    }
    if ["network", "drm", "audio"].contains(&kind) && r.contains_key("hardware_vendor") {
        r.remove("device");
    }
    if kind == "system" {
        r.remove("uptime");
    }
    r
}
pub fn compare(a: &Snapshot, b: &Snapshot) -> anyhow::Result<Comparison> {
    a.validate()?;
    b.validate()?;
    let mut d = Comparison {
        schema_version: 1,
        before_id: a.id.clone(),
        after_id: b.id.clone(),
        limitations: vec![],
        changes: vec![],
        findings: vec![],
    };
    if a.boot_id != b.boot_id {
        d . limitations . push ("Boot IDs differ: hotplug, enumeration and runtime state can differ independently of updates." . into ()) ;
    }
    if a.privilege != b.privilege {
        d.limitations.push(
            "Collector privileges differ; negative evidence requires complete sources.".into(),
        );
    }
    d.limitations.push(
        "Two observations cannot establish causality, source history or elapsed error rates."
            .into(),
    );
    for kind in a
        .collectors
        .keys()
        .chain(b.collectors.keys())
        .collect::<BTreeSet<_>>()
    {
        let sa = a.collectors.get(kind);
        let sb = b.collectors.get(kind);
        let complete = sa.is_some_and(|s| s.status == Status::Ok)
            && sb.is_some_and(|s| s.status == Status::Ok)
            && sa.zip(sb).is_some_and(|(x, y)| x.sources == y.sources);
        if kind == "journal" || !complete {
            d . changes . push (Change { collector : kind . clone () , identity : "source" . into () , kind : ChangeKind :: NotComparable , before : None , after : None , evidence : vec ! [format ! ("snapshot:{}/collectors/{kind}" , a . id) , format ! ("snapshot:{}/collectors/{kind}" , b . id)] , note : if kind == "journal" { "Journal windows/access differ or are not verified. Retained entries are observations only, never 'new errors'." } else { "At least one source is absent, incomplete or has a different selection scope; no disappearance or addition inferred." } . into () }) ;
        }
        if kind == "journal" {
            continue;
        }
        let (Some(sa), Some(sb)) = (sa, sb) else {
            continue;
        };
        let ia = index(kind, &sa.records);
        let ib = index(kind, &sb.records);
        for key in ia.keys().chain(ib.keys()).collect::<BTreeSet<_>>() {
            let va = ia.get(key);
            let vb = ib.get(key);
            if kind == "packages"
                && (va.is_some_and(|v| v.len() > 1) || vb.is_some_and(|v| v.len() > 1))
            {
                let versions = va
                    .into_iter()
                    .flatten()
                    .chain(vb.into_iter().flatten())
                    .map(|(_, r)| val(r, "version").to_string())
                    .collect::<BTreeSet<_>>();
                for version in versions {
                    let la = va
                        .into_iter()
                        .flatten()
                        .filter(|(_, r)| val(r, "version") == version)
                        .collect::<Vec<_>>();
                    let lb = vb
                        .into_iter()
                        .flatten()
                        .filter(|(_, r)| val(r, "version") == version)
                        .collect::<Vec<_>>();
                    let change = match (la.len(), lb.len()) {
                        (1, 1) => ChangeKind::Unchanged,
                        (1, 0) if complete => ChangeKind::Removed,
                        (0, 1) if complete => ChangeKind::Added,
                        _ => ChangeKind::NotComparable,
                    };
                    let mut evidence = vec![];
                    for (id, items) in [(&a.id, &la), (&b.id, &lb)] {
                        for (i, _) in items {
                            evidence.push(format!("snapshot:{id}/collectors/packages/records/{i}"));
                        }
                    }
                    d . changes . push (Change { collector : kind . clone () , identity : format ! ("{key};installed_version={version}") , kind : change , before : la . first () . map (| (_ , r) | (* r) . clone ()) , after : lb . first () . map (| (_ , r) | (* r) . clone ()) , evidence , note : "Multiple installed versions: exact-version set comparison, no upgrade/downgrade pairing." . into () }) ;
                }
                continue;
            }
            let ambiguous = va.is_some_and(|v| v.len() != 1) || vb.is_some_and(|v| v.len() != 1);
            let left = va.and_then(|v| v.first());
            let right = vb.and_then(|v| v.first());
            let (change, note) = if ambiguous {
                (
                    ChangeKind::NotComparable,
                    "Duplicate hardware signatures; identity ambiguous, not paired.",
                )
            } else {
                match (left, right) {
                    (Some((_, x)), Some((_, y))) => (
                        if meaningful(kind, x) == meaningful(kind, y) {
                            ChangeKind::Unchanged
                        } else {
                            ChangeKind::Changed
                        },
                        if kind == "packages" {
                            "Version differences are neutral version_changed; ordering not inferred."
                        } else {
                            "Positive observations; location renumbering alone is ignored where hardware identity is available."
                        },
                    ),
                    (Some(_), None) if complete => (
                        ChangeKind::Removed,
                        "Absent from complete later inventory, not proof of failure.",
                    ),
                    (None, Some(_)) if complete => (
                        ChangeKind::Added,
                        "Present only in complete later inventory.",
                    ),
                    _ => (
                        ChangeKind::NotComparable,
                        "Unmatched record with incomplete source; absence not inferred.",
                    ),
                }
            };
            let mut evidence = vec![];
            if let Some((i, _)) = left {
                evidence.push(format!("snapshot:{}/collectors/{kind}/records/{i}", a.id));
            }
            if let Some((i, _)) = right {
                evidence.push(format!("snapshot:{}/collectors/{kind}/records/{i}", b.id));
            }
            d.changes.push(Change {
                collector: kind.clone(),
                identity: key.clone(),
                kind: change,
                before: if ambiguous {
                    None
                } else {
                    left.map(|(_, r)| (*r).clone())
                },
                after: if ambiguous {
                    None
                } else {
                    right.map(|(_, r)| (*r).clone())
                },
                evidence,
                note: note.into(),
            });
        }
    }
    let kernel_change = d.changes.iter().find(|c| {
        c.collector == "system"
            && c.before
                .as_ref()
                .zip(c.after.as_ref())
                .is_some_and(|(x, y)| {
                    x.get("kernel_release")
                        .zip(y.get("kernel_release"))
                        .is_some_and(|(old, new)| old != new)
                })
    });
    for c in &d.changes {
        if c.kind == ChangeKind::Changed
            && a.collectors
                .get(&c.collector)
                .is_some_and(|s| s.status == Status::Ok)
            && b.collectors
                .get(&c.collector)
                .is_some_and(|s| s.status == Status::Ok)
            && ["pci", "network", "audio", "drm", "usb"].contains(&c.collector.as_str())
            && let (Some(x), Some(y)) = (&c.before, &c.after)
            && !val(x, "driver").is_empty()
            && val(y, "driver").is_empty()
        {
            let mut evidence = c.evidence.clone();
            if let Some(k) = kernel_change {
                evidence.extend(k.evidence.clone());
            }
            d . findings . push (Finding { rule_id : "LFR-DRIVER-001" . into () , category : c . collector . clone () , severity : "warning" . into () , confidence_basis : "The same unambiguous device signature is observed twice; a previously bound driver is absent. No causal test performed." . into () , observation : "A device lost its observed driver binding." . into () , hypothesis : if kernel_change . is_some () { "Possible driver initialization problem across a running-kernel change." } else { "Possible transient device/driver initialization problem." } . into () , evidence , alternatives : vec ! ["Hotplug, manual unbind, driver handoff or a collection race" . into ()] , limitations : vec ! ["Temporal correlation is not causality; no user-confirmed cause recorded." . into ()] , safe_checks : vec ! ["Inspect journalctl -b -k --priority=0..4 locally; review for private data before sharing." . into () , "Inspect the device's read-only driver symlink and installed firmware package versions." . into ()] }) ;
        }
        if c.collector == "services" && matches!(c.kind, ChangeKind::Added | ChangeKind::Changed) {
            d . findings . push (Finding { rule_id : "LFR-SERVICE-001" . into () , category : "services" . into () , severity : "warning" . into () , confidence_basis : "A failed-unit inventory observation changed; failure onset is not established." . into () , observation : "Failed system unit observed in later inventory." . into () , hypothesis : "Service failure may be relevant to the reported symptom." . into () , evidence : c . evidence . clone () , alternatives : vec ! ["Unrelated service, configuration or external resource failure" . into ()] , limitations : vec ! ["No ExecStart or environment collected; no automatic repair." . into ()] , safe_checks : vec ! ["Use systemctl status for the named unit locally; inspect its bounded journal window." . into ()] }) ;
        }
    }
    Ok(d)
}
