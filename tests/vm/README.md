# Disposable Debian VM validation (x86_64)

Run from the repository, as an ordinary host user. **Never run guest.py on the host.**
It refuses unless root, PID 1 is systemd, hostname is synthetic, and a per-run nonce
matches the cloud-init marker. The host runner itself must not be run as root.

```sh
cargo build --release --locked
mkdir -m 700 "$HOME/lfr-vm-validation"
scripts/test-vm.sh "$HOME/lfr-vm-validation"
```

Host prerequisites: Linux x86_64, Python >=3.12 (tested 3.13), glibc >=2.34,
OpenSSH client/keygen, GNU ar, tar support in Python, zstd, GNU timeout, HTTPS
access to archive.ubuntu.com and cloud.debian.org, at least 3 GiB free memory and
4 GiB free disk. No sudo, Docker, KVM, host package install or host systemd needed.
The runner extracts pinned Ubuntu QEMU packages **locally**, not via dpkg/apt.
Ubuntu package SHA256 and Debian image SHA512 are fixed in the two lock files.
Checksums pin bytes but do not replace signatures. The dated Debian image directory
provided no detached SHA512SUMS.sig/.sign (HTTP 404 during validation); HTTPS and
pinned checksum were verified, not an image signature. URLs can eventually expire;
cache the verified inputs in the private workdir. Do not silently change the locks.

Defaults: QEMU 6.2 TCG, q35, 2 vCPU, 1024 MiB RAM, 6 GiB sparse disposable qcow2
overlay over an unchanged base. Runtime CPU limit 1200 seconds per QEMU process;
AS limit 3 GiB; FSIZE limit 8 GiB; overall wall timeout 1800 seconds. These are
process/file bounds, not a filesystem quota or complete sandbox against hostile
images. Only the official pinned trusted image is used. No physical passthrough,
no writable shared directories. User-mode networking; SSH binds only 127.0.0.1
with a throwaway key. No secrets are required. The old pinned QEMU is a test
runtime, not a recommendation for production virtualization of untrusted guests.

`--prepare-only` verifies/downloads dependencies without boot. `--boot-only` boots,
checks SSH/cloud-init/systemd and powers off. `--accel kvm` is an explicit optional
choice; not tested here. The default TCG path was actually run.

The seed ISO is attached as a **read-only virtio block disk**, not an ATA CD-ROM:
the genericcloud kernel in this image did not enumerate the latter. PXE ROMs are
disabled for virtual NICs because no network boot is needed. On the second boot an
e1000 device is added without changing the management virtio NIC.

Run directories are unique, private and retained on success/failure. They contain
exact QEMU arguments, SSH and guest command exit codes/timings/stdout/stderr,
serial consoles, result.json, and guest-evidence.tar.gz. The guest archive contains
results.json, commands.json, saved snapshots/comparisons, Markdown and checked
public export. Review it before sharing. Overlay disks contain guest SSH host keys
and local journal data: **do not publish overlays**. SSH private/public login keys,
user-data and seed ISO are removed on normal exit/handled termination. SIGKILL,
host loss and power loss cannot run cleanup: inspect only the owned run directory.
The runner powers off then waits; escalation terminates only its own process group.
It never removes the workdir/cache/overlay or user files automatically.

The guest uses its own packaged Python/systemd/dpkg; test .debs have fixed versions
1.0 and 2.0 and only a harmless /usr/share file, no maintainer scripts. No distro
upgrade or apt dependency resolution is performed. The release binary is built on
the host, hashed, transferred over SSH and actually executed in Debian; this is not
proof of a native guest build or source-level bit reproducibility.

Read docs/validation.md for the executed attempt IDs, failures and remaining gaps.

Additional executed cleanup regression:

```sh
python3 tests/e2e/test_harness.py
python3 tests/e2e/test_cleanup.py "$HOME/lfr-vm-validation"
```

The second command starts a fresh owned QEMU, sends SIGTERM to its runner during
boot, and verifies non-success interruption status, no surviving QEMU, and removed
login private key/seed. It is not a guest power-loss/SQLite recovery test.
