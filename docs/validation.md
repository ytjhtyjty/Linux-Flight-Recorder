# Validation report

## Status and provenance

**Development preview — not production-ready. The project is under active development and may contain bugs.** Successful checks below are bounded evidence, not a guarantee of reliability, anonymity, or compatibility with every Linux distribution.

Validation was performed on October 29–30, 2026. The supplied working tree had no existing commit and contained untracked source files; therefore there is no upstream commit SHA to claim as a baseline. This publication is a separate, curated copy. Original source files and private test evidence were preserved outside this repository.

The initial validation preceded publication cleanup. Explanatory code comments and Python docstrings were subsequently removed; executable literals, shebangs and the cloud-init format marker were retained. Python executable ASTs were checked for equivalence. The final publication checks are recorded below. Binary hashes refer to the exact build named, not to all possible builds.

## Actual environments

- Host: Amazon Linux 2023, x86_64, kernel 6.18.49; unprivileged sandbox, 2 CPUs and 4 GiB memory. PID 1 was not systemd. Host system services, packages, network and boot configuration were not modified.
- No usable `/dev/kvm`; Docker and Podman were unavailable. QEMU **TCG**, not KVM, successfully booted a full guest with its own kernel and systemd.
- Guest: Debian 12 generic cloud amd64, image `20261026-2643`; kernel `6.1.0-54-cloud-amd64` (package `6.1.190-1`); systemd `252.39-1~deb12u2`.
- QEMU 6.2 and its pinned dependencies were extracted into a user-owned directory, not installed into the host. The harness uses q35, two virtual CPUs, 1 GiB guest RAM and a disposable 6 GiB sparse overlay. SSH forwards only to localhost. No physical devices or writable host shares are passed through.

Image source: <https://cloud.debian.org/images/cloud/bookworm/20261026-2643/debian-12-genericcloud-amd64-20261026-2643.qcow2>.

SHA-512, checked against the dated official HTTPS checksum manifest:

```text
0464b5d4ae3aa4f658468d88da83af7857b71548b30d46dcb6282770955a747fb95a983476db90d25afd0653af607224c7ebd05122ab71faa106fd712aaad652
```

Detached signature URLs were unavailable (HTTP 404); **no signature verification is claimed**. Runtime package URLs and digests are pinned in `tests/vm/`. Guest package versions and command results are recorded by each run; guest APT repositories are not an immutable historical package mirror.

## Results and scope

| Layer | Check | Status | Evidence and boundary |
| --- | --- | --- | --- |
| Unit | Rust tests after journal access fix | PASS | 57 tests; stable and minimum supported Rust 1.89 |
| Fixtures | Package adapters, parser and report/storage cases | PASS | Included in Rust suite; not equivalent to booting every supported distribution |
| Host harness | SSH readiness, bounded retry and interruption regressions | PASS | 5 Python tests |
| Build | Formatting, strict Clippy, debug and release builds | PASS | Original validated tree; publication rerun recorded below |
| Dependencies | cargo-audit and cargo-deny licenses/bans/sources | PASS | audit 0.22.2: 132 dependencies, no vulnerability findings at validation time; deny 0.18.3 allowed duplicate syn warning |
| Container/sandbox | Read-only real CLI collection and reports | PASS | Amazon Linux/RPM environment; not a system boot test |
| VM | Debian 12 complete E2E | PASS | Original run: 21/21 assertions, 114 recorded guest commands |
| VM | SIGTERM during guest boot | PASS | Intentional runner failure; QEMU terminated and temporary access keys removed |
| VM | Cold poweroff and relaunch | PASS | New boot ID, retained snapshots and boot collection |
| VM | Kernel upgrade | NOT RUN | Same kernel booted twice; installed and running kernel identities are distinct |
| VM | Live hotplug and device removal | NOT RUN | Only addition of a virtual PCI device between boots was exercised |
| VM | Fedora, Arch and Ubuntu full-system coverage | NOT RUN | Package fixtures and host adapter coverage do not establish this |
| Physical hardware | GPU, Wi-Fi, audio, firmware, suspend/resume | NOT RUN | A virtual PCI NIC is not a physical hardware certification |
| Reliability | Forced power loss during SQLite flush | NOT RUN | Process interruption and corruption were tested, not crash-consistency under power loss |
| CI/fuzz | Hosted CI and fuzz campaign | NOT RUN | No hosted runner or sustained fuzz campaign was executed |
| Product scope | Automatic repair and proven causal diagnosis | NOT APPLICABLE | Not implemented and not claimed |

The 21 VM scenarios combine real workflow checks with explicit fault injection. They exercise actual collectors and CLI commands, not a mock replacement for the recorder.

### Real VM workflow

- Baseline snapshot, persisted/readable snapshot, second snapshot, text/JSON comparison, Markdown report, export preview and tar archive contents.
- Locally built test packages: install version 1.0, upgrade to 2.0, then remove; assertions for added, version-changed and removed packages.
- A dedicated synthetic systemd service: successful start followed by controlled failure, without breaking existing services.
- Unique synthetic journal marker within the selected collection window. Restricted-user journal access followed by explicit journal group access; unavailable baseline data must not become a newly discovered regression.
- Separate root and user stores, permissions, locking, concurrent access, retention and pinned snapshots.
- Guest poweroff/relaunch with an additional virtual e1000 NIC; different boot ID, retained snapshots and real boot snapshot. This was not an in-place `systemctl reboot` test or a device renumbering/hotplug certification.
- Installation and removal of the project's systemd units in the disposable guest. Boot collection is tested; the daily timer is inspected, not observed for an entire day.

### Faults and adversarial data

- Missing journal command, explicit permission denial, hanging external process with a child, bounded stdout/stderr, unavailable storage and an 8 MiB **guest-only** tmpfs ENOSPC/recovery case.
- Interrupted collection does not publish a complete snapshot; expected interruption exit code 130. Timeouts must not leave uncontrolled child processes.
- Damaged SQLite data, unknown schema version, initial schema migration, lock contention and preservation of useful partial results.
- Synthetic hostname, home path, IP, MAC and token-like text; archive member validation, symlink/path traversal rejection, ANSI/control-character handling and safe human-readable output.
- Export redaction is best-effort. Arbitrary identity-like free text (for example `user=lfrtest`) may remain. Printable Unicode/bidirectional characters may remain in machine-readable data. **Manual review is required; anonymity is not guaranteed.**

### Fixed project defect

`journalctl` can exit 1 with empty stdout and the C-locale diagnostic `No journal files were opened due to insufficient permissions.` The collector previously classified this as generic failure. It now records `permission_denied`, retaining the diagnostic. The narrow match does not reclassify timeouts, unrelated failures or nonempty output.

Regression: `journalctl_access_denial_is_not_generic_failure`, including negative and timeout cases. The test failed before the fix (exit 101), passed afterwards, and the VM verifies the restricted-user path and rejects false “new error” claims after journal access changes.

### Remaining limitations and release gates

- Journal collection is visibly partial/noncomparable; binary journal MESSAGE arrays are deliberately omitted rather than silently treated as complete evidence.
- Sysfs deadlines are cooperative, not hard isolation from uninterruptible kernel reads.
- Logical snapshot quotas do not bound the physical size of SQLite, exports or caches.
- Device identity ambiguity, duplicate devices and renumbering need broader real-system validation.
- No full-system coverage beyond Debian 12 and no physical hardware or actual kernel-upgrade validation.
- No independently verified complete anonymization or production security/reliability certification.

These are reasons **not to label this a stable production release**. Wider distribution testing, hardware testing, crash-consistency testing and security/privacy review remain release gates.

## Reproduce

Run as an ordinary user on x86_64 Linux, with Python 3, Rust and the documented VM prerequisites:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
cargo build --release --locked
python3 -m unittest discover -s tests/e2e -p test_harness.py
python3 scripts/check_docs.py
scripts/test-vm.sh "$HOME/lfr-vm-validation" --binary "$PWD/target/release/lfr"
```

The work directory must be a private, user-owned, nonsymlink directory. See [VM harness](../tests/vm/README.md) and [E2E tests](../tests/e2e/README.md). The harness verifies the image, provisions a disposable guest, records commands/results and bounds execution time. Rerun with the same cache directory; each test run gets a separate overlay and artifact directory. Network access is needed for initial downloads and guest packages.

A successful VM run retains `result.json`, console/command logs and `guest-evidence.tar.gz`. Guest evidence includes `results.json` and `commands.json`; failures preserve evidence where possible. Temporary SSH private keys and cloud-init seeds are removed on cleanup. Treat raw evidence as private: it contains system diagnostic data. Images, binaries, raw logs, generated notices and credential files are intentionally not committed.

## Publication-copy verification

The cleaned publication tree was actually rebuilt and tested again. All commands below returned exit 0. Minimum-supported-Rust tests used a separate build directory and Rust 1.89; stable checks used Rust 1.99.1.

| Publication check | Status | Result |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | No formatting changes required |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS | No warnings |
| `cargo test --locked` (stable) | PASS | 57 tests |
| `cargo test --locked` (Rust 1.89) | PASS | 57 tests |
| `cargo build --locked` and `cargo build --release --locked` | PASS | Debug and release binaries built |
| `python3 -m unittest discover -s tests/e2e -p test_harness.py` | PASS | 5 tests |
| `python3 scripts/check_docs.py` | PASS | Required files, top-level links and example JSON |
| Python AST/TOML parsing and shell syntax | PASS | Published scripts parse; shell syntax accepted |
| `python3 scripts/smoke.py /absolute/path/to/lfr` | PASS | Real read-only host capture/compare/report/export/gc cycle |
| `scripts/test-vm.sh PRIVATE_WORKDIR --binary /absolute/path/to/lfr` | PASS | 21/21 scenarios: 16 VM workflow + 5 VM fault injection; 114 guest commands |
| `python3 tests/e2e/test_cleanup.py PRIVATE_WORKDIR` | PASS | SIGTERM during real QEMU boot; no surviving QEMU; keys and seed removed |
| `sh scripts/package.sh` | PASS | Candidate archive created; SHA-256 verification passed; not a published release |

Exact full VM run: `run-20261029T231325-05f794`. Cleanup regression: `run-20261029T231921-a89b11` (its internal runner intentionally exits 1; the cleanup assertion script exits 0).

SHA-256 of the release binary transferred and executed in the final full VM run:

```text
3f88029bd23e56e2104390a802eea1be288ea50ea81568e27c8fd38a8b305e9a
```

Comment-removal checks confirmed identical normalized non-documentation Rust tokens in 11 files and identical non-docstring Python executable ASTs in 9 files. Shebangs, literal strings, regexes and the cloud-init marker were preserved. Legal license text was not stripped.

Publication changes also include English development-status documentation, the repository URL and artifact exclusions. The documentation checker now checks the curated published file set; the package script no longer references an omitted optional code-of-conduct file. Its archive path was exercised successfully after that change. Dependency audit results in the main table are from the earlier validation; the lockfile was unchanged during publication cleanup.

Raw machine diagnostics, generated binary packages, downloaded images, generated dependency-license directories and temporary credentials are excluded from Git. Source-only notice-generation instructions are included. Private run artifacts retain exact commands, exit codes and logs; they are not automatically safe to publish. No GitHub Actions success is claimed: hosted workflows are not included in this publication.
