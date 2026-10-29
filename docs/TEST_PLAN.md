# Verification plan

## Automated local tests
`cargo test --locked --all-targets`; PROPTEST_CASES defaults are supplied by
proptest (environment override PROPTEST_CASES spelled without the space).
Golden files are checked, never automatically regenerated in CI. Parser fixtures:
Arch, Debian/Ubuntu and RPM. Failure tests include malformed/truncated/non-UTF8,
missing permissions, source disappearance, unavailable source, no systemd,
unknown distro, output cap, timeout, cancellation, symlink, lock, integrity,
transaction rollback, SQLite simulated FULL, retention and pin. /dev/full verifies
real ENOSPC semantics; bounded filesystem-full recovery is also exercised in the disposable VM.

`python3 scripts/smoke.py target/release/lfr` creates two real snapshots in a
new isolated temporary private store, compares, reports, previews, exports and gc.
It does NOT update packages, install units or mutate hardware. Run separately per
target distro as a non-root user; development containers prove only accessible sources.

## VM smoke matrix — partially executed
Debian 12 was exercised in the full disposable VM harness; see [executed results](validation.md).
The remaining matrix is a plan, not a claim of successful tests: fresh Arch,
Ubuntu LTS and Fedora stable; regular user and explicit
system mode. Build/test from source. Run the smoke script before installing units.
In the disposable VM only, install optional units, inspect
`systemd-analyze verify systemd/*.service systemd/*.timer`, start once, check
`journalctl -u lfr-boot.service`, permissions and exit 3 handling. Verify that
hardening permits required proc/sysfs/journal reads. Do not enable on host yet.
Capture baseline; update packages only with explicit approval inside disposable
VM; capture after installation and after reboot separately. Compare installed and
running kernel. Verify interrupted SQLite write with kill -9, recovery on reopening,
OS disk quota/ENOSPC, free-space restoration and pinned over-limit warnings.
A VM cannot validate all real Wi-Fi/audio/GPU/USB effects.

## Real hardware — NOT executed
Record distro/kernel/device/driver versions and permissions. Baseline with Wi-Fi,
audio, external monitor and USB dock. With approval, compare safe hotplug and
sleep/wake cycles; independently verify observations in sysfs. Then an approved
normal update plus reboot, preserving the baseline. Never unbind PCI, write MMIO,
change clock/voltage, load/unload modules or automate rollback. Review private data
before exporting. Confirm duplicate identical devices are marked ambiguous.

## Performance
Method: release binary, isolated store, wall time and child peak RSS from Python
resource (Linux ru_maxrss, KiB, high-water across children), serialized bytes and
requested journal bytes; compare time on the same two saved snapshots. No synthetic
hardware benchmark claims. See [validation](validation.md) for the actual environment and evidence scope.
