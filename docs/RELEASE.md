# Development status and release gates

**0.1.0 is unreleased development software. It may contain bugs and is not
production-ready.** Publishing this source is not a stable release or certification.

The Debian 12 x86_64 QEMU TCG workflow was actually tested; see
[validation](validation.md). The following remain open:

- Full Ubuntu, Arch and Fedora validation, plus any additional supported Debian images.
- Physical GPU, Wi-Fi, audio, firmware, suspend/resume and driver/hardware tests.
- A real running-kernel upgrade, virtual device removal and live hotplug validation.
- Hard isolation of kernel-blocked filesystem/sysfs reads.
- Hard physical storage bounds beyond logical JSON retention quotas.
- Forced power loss during SQLite flush and recovery across different filesystems.
- Long-running periodic timer behavior, a fuzz campaign and actual hosted CI results.
- Privacy/Unicode policy hardening and a verified private security reporting channel.
- Final legal/notices review, release signatures/provenance and maintainer release policy.

Do not advertise package history or automatic transaction hooks: neither is shipped.
Do not extrapolate parser fixtures or a VM test to physical hardware support.

Before a future release, review Cargo.lock, run fmt/clippy/tests on stable and MSRV,
rerun the relevant VM matrix, review dependency advisories and legal notices,
collect original license texts, generate the SBOM, and inspect reports for privacy.
Use `scripts/package.sh` for a candidate archive only. SHA256 verifies byte
consistency, not author identity; no signing or bit-reproducibility claim is made.
