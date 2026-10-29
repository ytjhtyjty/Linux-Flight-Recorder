# Architecture

One crate exposes a library and lfr binary. No unnecessary crate graph or async runtime.

- model: typed snapshot/result/config/status, schema compatibility.
- collect: Sources injection for bounded sysfs/proc and Runner injection for commands;
  package adapters and journal parsers operate on bytes and reject malformed frames.
- runner: trusted executable lookup, no shell, clean environment, nonblocking pipes,
  combined caps, process-group cleanup, timeout and cancellation.
- storage: private paths and flock, SQLite migration 0->1, JSON+hash+index atomic
  transaction. Crash recovery delegated to SQLite FULL synchronous rollback journaling.
- diff: snapshot-only identity matching; no live calls. BTree maps stabilize order.
  Evidence pointers preserve exact original record positions. Duplicate hardware signatures
  are not paired; multi-version packages use exact-version set membership. Unmatched records need complete sources to infer added/removed.
- privacy: allowlisted collection first, known-context/pattern redaction second,
  separate public projection; report rendering escapes active text.
- report: uses only provided snapshots/comparison. Bounded Markdown summaries,
  full comparison JSON, fixed-member deterministic tar/gzip export.
- CLI: owns paths/config/store, exit codes and signals. No collectors print stdout.

IDs: 128 random bits from /dev/urandom, lower hex; labels are not identity.
Clock: collection start and gc reference time are explicit inputs; finish timestamp
is real UTC wall clock. Full virtual-clock finish injection is not implemented.
Sources abstracts filesystem reads; Runner abstracts command execution. Storage
fault tests use SQLite limits and explicit failure callbacks, not destructive fills.

No stored raw command output or external artifact tree. This is an intentional
scope choice; the snapshot JSON is the diagnostic artifact. See ADR 0001.
