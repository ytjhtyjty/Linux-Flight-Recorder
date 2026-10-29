# JSON contract version 1

Snapshot fields: schema_version, application_version, id, started_at, finished_at,
boot_id (optional), source, label, completeness, privilege, collectors. Times are
RFC3339 with explicit +00:00. Each named collector has status, sources, limitations,
records (string-valued typed-domain fields) and duration_ms.
Status: ok, partial, unavailable, permission_denied, timeout, failed. Never treat a
non-ok empty records array as negative evidence. Unknown JSON fields are tolerated
when reading snapshots, but a newer schema_version is rejected. Configuration
rejects unknown fields. SQLite user_version is independent of snapshot version.

Comparison: schema_version=1, before_id, after_id, limitations, changes, findings.
Change kind: added, removed, changed, unchanged, not_comparable. Evidence pointers:
`snapshot:ID/collectors/NAME/records/INDEX`; collector-level pointers omit records.
Findings have stable rule_id, category, severity, confidence_basis, observation,
hypothesis, evidence, alternatives, limitations and safe_checks. No numeric
confidence, auto-confirmed cause or executable fix is emitted.
Multiple installed versions of one package are matched as exact-version sets, not
arbitrarily paired upgrades. Package version direction is never inferred. stack_group is a heuristic display
classification (kernel/firmware/graphics/other), not causal evidence.

Journals: exact retained scalar MESSAGE/unit/transport/priority/cursor/timestamp/
boot fields; non-scalars visibly omitted; no message normalization or grouping in
0.1 and no novelty/frequency claims. Windows and access limitations are retained.

Comparison ordering is deterministic for identical stored inputs. Reordered input
records retain different evidence indices by design, but semantic change ordering
is stable. Snapshot bytes differ by ID, times and duration; do not expect two live
collections to be byte-identical. Golden fixtures define canonical JSON/Markdown.
