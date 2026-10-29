# Threat model and acceptance scope
Untrusted device strings, journal records, labels, corrupted snapshots and command
output must not become shell code, terminal escape sequences, Markdown directives,
file paths or export archive members. Paths from logs are never followed. Secrets
are excluded structurally first; heuristic redaction is additional, not anonymity.
Store must be owned by effective uid, private, non-symlink components. Do not use
shared/group-writable storage, sudo-preserved XDG paths, or mix system/user stores.
Root opts in with --system; root is not needed for user-mode collection.
DoS threats: large output, hangs, concurrent writers, storage exhaustion and damaged
DB. Hard caps, process groups, lock, quotas, transactions and size checks mitigate
these. A malicious same-uid process can alter the store; it is outside the trust
boundary. SHA-256 detects accidental corruption, not authenticity.
Acceptance for 0.1: executable vertical scenario with partial-source semantics,
strict schema rejection, deterministic reports, export preview, retention and tests.
Not a stable release: mandatory target-distro end-to-end validation remains pending.
