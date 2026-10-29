# Linux Flight Recorder

**Development preview (0.1.0). This project is under active development, may contain
bugs, and is not production-ready. Back up important data and test in a disposable
environment before relying on it. No stable support or complete hardware coverage
is claimed.**

Linux Flight Recorder is a local, offline diagnostic tool for answering **what
changed?** Capture a working system, capture it again after a problem, and compare
saved evidence. It does not automatically repair the system or treat correlation
as proof that an update caused a failure.

## What it does

- Saves minimal system, running-kernel and boot information.
- Collects dpkg, RPM or pacman package inventories when the matching adapter is available.
- Collects accessible PCI, USB, DRM, network, rfkill and ALSA/sysfs information.
- Records failed system units, selected systemd service properties and bounded journal entries.
- Compares snapshots offline, with conservative device matching and evidence references.
- Produces text/JSON comparisons, Markdown reports and previewable tar.gz exports.
- Stores snapshots in private SQLite storage with integrity checks, retention and pins.

Incomplete or inaccessible sources stay explicit. An unreadable source is not
proof that devices disappeared. Journal entries are observations, not proven new
errors. Findings separate observations, hypotheses, alternatives and limitations.
There is no telemetry, mandatory API, persistent daemon, automatic sudo invocation,
package-manager hook or automatic repair. Package history is not implemented.

## Build

Linux, Rust >=1.89 and a C compiler/linker are required. SQLite is built from
bundled source by the dependency. Cargo needs network access unless dependencies
are already cached or vendored. Python is used for tests/tooling, not by the CLI.

```sh
cargo build --release --locked
./target/release/lfr --help
./target/release/lfr doctor
./target/release/lfr config validate
```

See [installation and removal](INSTALL.md) and [configuration](CONFIGURATION.md).
Do not install the binary setuid. Check for an existing program named `lfr` before
installing anything. Root storage requires the explicit `--system` flag.

## Basic workflow

```sh
./target/release/lfr snapshot create --label before-update
./target/release/lfr snapshot list
./target/release/lfr snapshot create --label after-update
./target/release/lfr compare BEFORE_ID AFTER_ID
./target/release/lfr compare BEFORE_ID AFTER_ID --format json
./target/release/lfr report BEFORE_ID AFTER_ID --symptom wifi --output report.md
./target/release/lfr export AFTER_ID --preview
./target/release/lfr export AFTER_ID --output bundle.tar.gz
./target/release/lfr snapshot pin BEFORE_ID
./target/release/lfr gc --dry-run
```

Replace placeholders with the returned 32-character snapshot IDs, not labels.
**Exit 3 means a partial snapshot was successfully saved**, not a complete failure.
Other exits: 0 success, 1 runtime/storage failure, 2 CLI/config error, 130 interrupted.
Do not chain snapshot capture with plain `&&` when partial results are acceptable.
Existing output files are never overwritten. Stores and output parents must use
physical paths, not symlinks. User and system stores are separate.

## How it was tested

Actual validation included Rust tests on stable and the minimum supported Rust,
formatting, clippy, debug/release builds, dependency checks, real read-only Linux
collection, and a fully booted Debian 12 x86_64 VM using rootless QEMU TCG (no KVM).
The VM used its own 6.1.0-54-cloud-amd64 kernel and systemd, not the host kernel.

The VM scenarios exercised real dpkg install/version-change/removal, a synthetic
service entering failed state, journald access changes, report/export checks,
bounded failures, an isolated full tmpfs, retention/pins, a new boot ID, boot timer,
virtual PCI addition and uninstall. See the dated [validation report](docs/validation.md)
for exact results, commands, image checksums and what was **not** tested.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
python3 tests/e2e/test_harness.py
cargo build --release --locked
scripts/test-vm.sh "$HOME/lfr-vm-validation"
```

The VM runner downloads pinned official inputs and keeps test artifacts in a
private directory. It does not install packages or change services on the host.
Read [VM prerequisites and safety](tests/vm/README.md) first. Never run the guest
runner directly on a workstation. GitHub-hosted CI results are not claimed.

## Validation boundaries

| Area | Evidence | Not implied |
|---|---|---|
| Debian 12 amd64 | Actual QEMU TCG workflow and fault tests | Every Debian release, Ubuntu, or physical hardware |
| RPM | Parser fixtures and real Amazon Linux container inventory | Full Fedora update/boot validation |
| pacman | Parser fixtures | An actual Arch system test |
| Device matching | Fixtures for duplicates/renumbering; virtual PCI addition | Physical GPU/Wi-Fi/audio, live hotplug or suspend/resume |
| Kernel data | Installed package and running-kernel fields recorded separately | A real kernel upgrade; the same kernel was booted twice |
| Privacy | Synthetic pattern/identity/export tests | Complete anonymization or safe publication without review |

## Known limitations

Filesystem/sysfs read deadlines are cooperative, not hard isolation of a
kernel-blocked read. Logical snapshot quotas do not cap all physical SQLite,
export or cache usage. Abrupt power loss during a database flush, physical hardware,
other full target distributions and long-running periodic timers remain unverified.

Exports use best-effort redaction. Arbitrary other usernames can remain in logs;
printable Unicode/bidi remains JSON data. Review every report/archive before
sharing. See [privacy](PRIVACY.md), [security](SECURITY.md), [threat model](docs/THREAT_MODEL.md)
and [release gates](docs/RELEASE.md). The shipped systemd automation is experimental.

This repository contains source, tests and reproducible tooling, not binaries,
VM disks, private keys, raw machine snapshots or large validation logs. Dependency
licenses/notices must be collected before redistributing binary packages; see
[third-party notices](THIRD_PARTY_NOTICES.md). Source license: [Apache-2.0](LICENSE).
