# Installation and removal

**Development software: may contain bugs and is not production-ready.** This is a
source-only repository; this upload does not publish an official binary release.
A host-built x86_64 glibc binary was actually executed in Debian 12 TCG validation,
not certified for every Linux system. See [validation](docs/validation.md).

## Build and user installation

Rust >=1.89, a C compiler and linker, and Linux are required. Python is only needed
for verification tooling. Cargo dependencies must be cached/vendored for offline builds.

```sh
cargo test --locked --all-targets
cargo build --release --locked
command -v lfr
```

Inspect the destination first. Do not overwrite another program using this name.
Choose another binary path/name and adjust commands/units if there is a conflict.

```sh
install -Dm755 target/release/lfr "$HOME/.local/bin/lfr"
install -Dm644 man/lfr.1 "$HOME/.local/share/man/man1/lfr.1"
install -Dm644 completions/lfr.bash "$HOME/.local/share/bash-completion/completions/lfr"
```

Never setuid the binary. Add `$HOME/.local/bin` to your own PATH if necessary.
For offline source transfer, run `cargo vendor --locked vendor` on a connected
machine and configure Cargo source replacement. Dependencies are not vendored here.

## Optional system installation

Only with explicit administrator approval:

```sh
install -Dm755 target/release/lfr /usr/local/bin/lfr
```

Read [systemd instructions](systemd/README.md) before enabling experimental units.
The test runner installs them only inside its nonce-marked disposable guest.
It never installs them on the host. No package-manager hooks are shipped.

## Removal

1. If enabled by the administrator, disable `lfr-boot.timer` and `lfr-periodic.timer`,
   stop LFR services, remove only the four installed LFR unit files and run
   `systemctl daemon-reload`.
2. Remove the binary, manual and completion at the exact paths you selected.
3. Inspect `${XDG_STATE_HOME:-$HOME/.local/state}/lfr`, then optionally remove that
   exact directory, including index.sqlite3, lock and SQLite recovery files.
4. Inspect and optionally remove `/var/lib/lfr` as administrator, plus explicit
   alternate stores, configuration, reports and exports you created.

Do not use blanket deletion globs or commands copied from untrusted reports.
There is no hidden cloud data or telemetry to remove.
