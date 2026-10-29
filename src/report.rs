use crate::{
    diff::{ChangeKind, Comparison},
    model::Snapshot,
    privacy::{public_snapshot, redact, visible},
    storage,
};
use anyhow::Result;
use std::{collections::BTreeMap, path::Path};
fn safe(s: &str) -> String {
    visible(&redact(s, &[]))
}
pub fn markdown(a: &Snapshot, b: &Snapshot, d: &Comparison, symptom: Option<&str>) -> String {
    let important = d
        .changes
        .iter()
        .filter(|c| !matches!(c.kind, ChangeKind::Unchanged | ChangeKind::NotComparable))
        .count();
    let mut s = format!(
        "# Linux Flight Recorder report\n\n## 1. Summary\n\n{important} observed changes; {} hypotheses. No root cause is established.\n\nSymptom: {} (user-supplied context, not a causal filter).\n\n## 2. Compared snapshots\n\n| State | ID | Collected (explicit UTC offset) | Label |\n|---|---|---|---|\n| Before | {} | {} | {} |\n| After | {} | {} | {} |\n\n",
        d.findings.len(),
        safe(symptom.unwrap_or("unspecified")),
        a.id,
        safe(&a.started_at),
        safe(&a.label),
        b.id,
        safe(&b.started_at),
        safe(&b.label)
    );
    s.push_str("## 3. Completeness and limitations\n\n");
    for (name, snap) in [("Before", a), ("After", b)] {
        for (kind, set) in &snap.collectors {
            s.push_str(&format!(
                "- {name} / {}: {:?}; {} records.\n",
                safe(kind),
                set.status,
                set.records.len()
            ));
            for note in &set.limitations {
                s.push_str(&format!("  - {}\n", safe(note)));
            }
        }
    }
    for l in &d.limitations {
        s.push_str(&format!("- {}\n", safe(l)));
    }
    s . push_str ("\n## 4. Significant changes\n\nInstalled package versions and the running kernel are different concepts. Package changes do not prove a regression. All version differences are neutral version_changed.\n\n") ;
    let packages = d
        .changes
        .iter()
        .filter(|c| {
            c.collector == "packages"
                && !matches!(c.kind, ChangeKind::Unchanged | ChangeKind::NotComparable)
        })
        .collect::<Vec<_>>();
    for c in packages.iter().take(30) {
        let old = c
            .before
            .as_ref()
            .and_then(|r| r.get("version"))
            .map(String::as_str)
            .unwrap_or("absent");
        let new = c
            .after
            .as_ref()
            .and_then(|r| r.get("version"))
            .map(String::as_str)
            .unwrap_or("absent");
        let group = c
            .after
            .as_ref()
            .or(c.before.as_ref())
            .and_then(|r| r.get("stack_group"))
            .map(String::as_str)
            .unwrap_or("other");
        s.push_str(&format!(
            "- Stack group {}: {} -> {}\n",
            safe(group),
            safe(old),
            safe(new)
        ));
        s.push_str(&format!("- {:?}: {}\n", c.kind, safe(&c.identity)));
        for e in &c.evidence {
            s.push_str(&format!("  - Evidence: {}\n", safe(e)));
        }
    }
    if packages.len() > 30 {
        s.push_str(&format!(
            "- {} additional package changes omitted from Markdown; use compare --format json.\n",
            packages.len() - 30
        ));
    }
    s.push_str("\n## 5. Devices and services\n\n");
    for c in d
        .changes
        .iter()
        .filter(|c| c.collector != "packages" && c.kind != ChangeKind::Unchanged)
        .take(150)
    {
        s.push_str(&format!(
            "- {} / {:?}: {} — {}\n",
            safe(&c.collector),
            c.kind,
            safe(&c.identity),
            safe(&c.note)
        ));
        for (label, r) in [("Before", &c.before), ("After", &c.after)] {
            if let Some(r) = r {
                s.push_str(&format!(
                    "  - {label}: {}\n",
                    safe(&serde_json::to_string(r).unwrap_or_default())
                ));
            }
        }
        for e in &c.evidence {
            s.push_str(&format!("  - Evidence: {}\n", safe(e)));
        }
    }
    s . push_str ("\n## 6. Journal observations\n\nNot classified as new errors or changed error rates: access and window coverage are not established. Up to 20 records per snapshot shown below.\n\n") ;
    for snap in [a, b] {
        if let Some(set) = snap.collectors.get("journal") {
            for (i, r) in set.records.iter().take(20).enumerate() {
                s.push_str(&format!(
                    "- Evidence snapshot:{}/collectors/journal/records/{i}: {}\n",
                    snap.id,
                    safe(
                        r.get("message")
                            .map(String::as_str)
                            .unwrap_or("[no message]")
                    )
                ));
            }
        }
    }
    s.push_str("\n## 7. Explainable hypotheses\n\n");
    if d.findings.is_empty() {
        s.push_str("No configured rule matched. This does not prove absence of a problem.\n");
    }
    for f in &d.findings {
        s.push_str(&format!(
            "### {} ({})\n\nObservation: {}\n\nHypothesis: {}\n\nConfidence basis: {}\n\n",
            safe(&f.rule_id),
            safe(&f.severity),
            safe(&f.observation),
            safe(&f.hypothesis),
            safe(&f.confidence_basis)
        ));
        for (title, items) in [
            ("Evidence", &f.evidence),
            ("Alternatives", &f.alternatives),
            ("Limitations", &f.limitations),
        ] {
            for item in items {
                s.push_str(&format!("- {title}: {}\n", safe(item)));
            }
        }
    }
    s . push_str ("\n## 8. Safe additional checks\n\nNothing is executed automatically. Do not run arbitrary commands from logs.\n\n") ;
    for f in &d.findings {
        for step in &f.safe_checks {
            s.push_str(&format!("- {}\n", safe(step)));
        }
    }
    s . push_str ("- Review read-only local journal windows and device bindings; preserve a known-good manual baseline before an update.\n\n## 9. Privacy transformations\n\nSerial numbers, EDID, hostname, addresses, SSID, process environments and kernel command line are not collected as structured fields. Known local identity strings and obvious address/home/token patterns are redacted. Public bundles omit boot IDs and journal cursors. Journal text can still contain personal information: manually review before publication. Redaction is not anonymity.\n\n## 10. Versions\n\n") ;
    s . push_str (& format ! ("Report JSON schema {}; before app {} / snapshot schema {}; after app {} / snapshot schema {}.\n" , d . schema_version , safe (& a . application_version) , a . schema_version , safe (& b . application_version) , b . schema_version)) ;
    s
}
pub fn terminal(d: &Comparison) -> String {
    let mut counts = BTreeMap::new();
    for c in &d.changes {
        *counts.entry(format!("{:?}", c.kind)).or_insert(0) += 1;
    }
    format!(
        "{} -> {}\n{}\nHypotheses: {}. No causal conclusion. Use --format json for full evidence.\n",
        d.before_id,
        d.after_id,
        counts
            .into_iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join(", "),
        d.findings.len()
    )
}
pub const MEMBERS: [&str; 3] = ["snapshot.json", "manifest.json", "PRIVACY.txt"];
pub fn preview(s: &Snapshot) -> serde_json::Value {
    serde_json :: json ! ({ "schema_version" : 1 , "snapshot_id" : s . id , "files" : MEMBERS , "transformations" : ["omit boot IDs and journal cursors" , "redact known patterns; no raw command output"] , "warning" : "Manually review journal and labels. No guarantee of anonymity." })
}
pub fn export(s: &Snapshot, path: &Path) -> Result<()> {
    let public = serde_json::to_vec_pretty(&public_snapshot(s))?;
    let manifest = serde_json::to_vec_pretty(
        &serde_json :: json ! ({ "schema_version" : 1 , "files" : [{ "path" : "snapshot.json" , "sha256" : storage :: hash (& public) , "bytes" : public . len () } , { "path" : "PRIVACY.txt" , "sha256" : storage :: hash (PRIVACY . as_bytes ()) , "bytes" : PRIVACY . len () }] }),
    )?;
    storage::write_output(path, |file| {
        let gz = flate2::GzBuilder::new()
            .mtime(0)
            .write(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(gz);
        for (name, b) in [
            (MEMBERS[0], public.as_slice()),
            (MEMBERS[1], manifest.as_slice()),
            (MEMBERS[2], PRIVACY.as_bytes()),
        ] {
            let mut h = tar::Header::new_gnu();
            h.set_size(b.len() as u64);
            h.set_mode(0o600);
            h.set_mtime(0);
            h.set_uid(0);
            h.set_gid(0);
            h.set_cksum();
            tar.append_data(&mut h, name, b)?;
        }
        let gz = tar.into_inner()?;
        gz.finish()?;
        Ok(())
    })
}
const PRIVACY: &str = "Review all files before public sharing. Redaction is best-effort, not anonymity. Logs can contain arbitrary personal data. No source files from log paths were read. Boot IDs and journal cursors were excluded from this bundle.\n";
