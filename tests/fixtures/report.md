# Linux Flight Recorder report

## 1. Summary

5 observed changes; 2 hypotheses. No root cause is established.

Symptom: wifi (user-supplied context, not a causal filter).

## 2. Compared snapshots

| State | ID | Collected (explicit UTC offset) | Label |
|---|---|---|---|
| Before | 00000000000000000000000000000001 | 2026-10-29T12:00:00+00:00 | fixture |
| After | 00000000000000000000000000000002 | 2026-10-29T12:00:00+00:00 | fixture |

## 3. Completeness and limitations

- Before / journal: Unavailable; 0 records.
  - access unavailable
- Before / packages: Ok; 2 records.
- Before / pci: Ok; 1 records.
- Before / services: Ok; 0 records.
- Before / system: Ok; 1 records.
- After / journal: Partial; 1 records.
- After / packages: Ok; 2 records.
- After / pci: Ok; 1 records.
- After / services: Ok; 1 records.
- After / system: Ok; 1 records.
- Boot IDs differ: hotplug, enumeration and runtime state can differ independently of updates.
- Two observations cannot establish causality, source history or elapsed error rates.

## 4. Significant changes

Installed package versions and the running kernel are different concepts. Package changes do not prove a regression. All version differences are neutral version_changed.

- Stack group firmware: 1 -> 2
- Changed: adapter=rpm;name=firmware;architecture=x
  - Evidence: snapshot:00000000000000000000000000000001/collectors/packages/records/0
  - Evidence: snapshot:00000000000000000000000000000002/collectors/packages/records/0
- Stack group kernel: 6.1 -> 6.2
- Changed: adapter=rpm;name=kernel;architecture=x
  - Evidence: snapshot:00000000000000000000000000000001/collectors/packages/records/1
  - Evidence: snapshot:00000000000000000000000000000002/collectors/packages/records/1

## 5. Devices and services

- journal / NotComparable: source — Journal windows/access differ or are not verified. Retained entries are observations only, never 'new errors'.
  - Evidence: snapshot:00000000000000000000000000000001/collectors/journal
  - Evidence: snapshot:00000000000000000000000000000002/collectors/journal
- pci / Changed: vendor=0x8086;device=0x1234;subsystem\u{5f}vendor=;subsystem\u{5f}device=;class=0x028000 — Positive observations; location renumbering alone is ignored where hardware identity is available.
  - Before: {"class":"0x028000","device":"0x1234","driver":"iwlwifi","location":"0000:00:01.0","vendor":"0x8086"}
  - After: {"class":"0x028000","device":"0x1234","location":"0000:00:01.0","vendor":"0x8086"}
  - Evidence: snapshot:00000000000000000000000000000001/collectors/pci/records/0
  - Evidence: snapshot:00000000000000000000000000000002/collectors/pci/records/0
- services / Added: name=wifi.service — Present only in complete later inventory.
  - After: {"active":"failed","name":"wifi.service"}
  - Evidence: snapshot:00000000000000000000000000000002/collectors/services/records/0
- system / Changed: ID=debian — Positive observations; location renumbering alone is ignored where hardware identity is available.
  - Before: {"ID":"debian","kernel\u{5f}release":"6.1"}
  - After: {"ID":"debian","kernel\u{5f}release":"6.2"}
  - Evidence: snapshot:00000000000000000000000000000001/collectors/system/records/0
  - Evidence: snapshot:00000000000000000000000000000002/collectors/system/records/0

## 6. Journal observations

Not classified as new errors or changed error rates: access and window coverage are not established. Up to 20 records per snapshot shown below.

- Evidence snapshot:00000000000000000000000000000002/collectors/journal/records/0: driver init failed \u{3c}untrusted\u{3e} \u{5b}redacted\u{5d} \u{5b}redacted\u{5d}

## 7. Explainable hypotheses

### LFR-DRIVER-001 (warning)

Observation: A device lost its observed driver binding.

Hypothesis: Possible driver initialization problem across a running-kernel change.

Confidence basis: The same unambiguous device signature is observed twice; a previously bound driver is absent. No causal test performed.

- Evidence: snapshot:00000000000000000000000000000001/collectors/pci/records/0
- Evidence: snapshot:00000000000000000000000000000002/collectors/pci/records/0
- Evidence: snapshot:00000000000000000000000000000001/collectors/system/records/0
- Evidence: snapshot:00000000000000000000000000000002/collectors/system/records/0
- Alternatives: Hotplug, manual unbind, driver handoff or a collection race
- Limitations: Temporal correlation is not causality; no user-confirmed cause recorded.
### LFR-SERVICE-001 (warning)

Observation: Failed system unit observed in later inventory.

Hypothesis: Service failure may be relevant to the reported symptom.

Confidence basis: A failed-unit inventory observation changed; failure onset is not established.

- Evidence: snapshot:00000000000000000000000000000002/collectors/services/records/0
- Alternatives: Unrelated service, configuration or external resource failure
- Limitations: No ExecStart or environment collected; no automatic repair.

## 8. Safe additional checks

Nothing is executed automatically. Do not run arbitrary commands from logs.

- Inspect journalctl -b -k --priority=0..4 locally; review for private data before sharing.
- Inspect the device's read-only driver symlink and installed firmware package versions.
- Use systemctl status for the named unit locally; inspect its bounded journal window.
- Review read-only local journal windows and device bindings; preserve a known-good manual baseline before an update.

## 9. Privacy transformations

Serial numbers, EDID, hostname, addresses, SSID, process environments and kernel command line are not collected as structured fields. Known local identity strings and obvious address/home/token patterns are redacted. Public bundles omit boot IDs and journal cursors. Journal text can still contain personal information: manually review before publication. Redaction is not anonymity.

## 10. Versions

Report JSON schema 1; before app 0.1.0 / snapshot schema 1; after app 0.1.0 / snapshot schema 1.
