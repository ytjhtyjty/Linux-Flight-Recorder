# Security policy

**This is an experimental development project. Bugs may exist; no supported stable
release or security certification is claimed.** Do not rely on it as a privileged
security boundary or recovery mechanism.

Do not publish real tokens, private diagnostic archives or identifying logs in
issues. No private vulnerability-reporting channel has been verified for this
initial upload. Contact the repository owner through a trusted private channel
before sharing sensitive details; use public issues only for nonsensitive reports.
Rotate any exposed credentials immediately.

Never install setuid, execute report-provided commands, or use a shared store.
External commands use fixed trusted paths, no shell, a clean environment, bounded
output/time and process-group cleanup. Same-UID attackers are outside the storage
isolation boundary. SQLite hashes are integrity checks, not signatures.

Kernel-blocked sysfs reads still need hard isolation; logical quotas do not cap all
physical disk usage. Redaction is best-effort, not anonymity. Review all exports.
See [threat model](docs/THREAT_MODEL.md), [privacy](PRIVACY.md), and
[release gates](docs/RELEASE.md). Report exact versions and a minimal synthetic
reproducer whenever possible; never upload real secrets to demonstrate a bug.
