# E2E assertions and expected evidence

Actual executable runner: `tests/vm/guest.py`, invoked only through
`scripts/test-vm.sh PRIVATE_WORKDIR`. Tests do not substitute fixtures for successful
real collection. Results labelled **VM fault injection** intentionally remove or
replace journalctl temporarily, restore it in finally, and are not real journal
functionality tests. Existing Rust parser/identity/permission/crash tests remain
separate unit/integration fixtures in `tests/core.rs`.

| Controlled scenario | Expected comparison / evidence | Forbidden conclusion |
|---|---|---|
| Install synthetic .deb 1.0 | packages added, real dpkg-query record | Installed means running software/kernel changed |
| Replace by .deb 2.0 | same name/arch changed version 1.0 -> 2.0 | Lexical version ordering proves downgrade/upgrade semantics |
| dpkg purge | packages removed | Missing collector means removed package |
| Own unit true -> false | failed unit added, selected active=failed, LFR-SERVICE-001 | Package/kernel change caused service failure |
| logger unique err + flush | marker retained in bounded current-boot journal | Error is novel, rate increased, baseline had full coverage |
| Same user gains journal group | later journal readable, journal not_comparable | Previously inaccessible entries are new errors |
| Cold poweroff/start | different boot ID, previous snapshots load | Real kernel upgrade or physical restart tested |
| Add emulated e1000 on second boot | PCI 8086:100e added, management NIC persists | Physical GPU/Wi-Fi/hotplug/renumbering validated |
| Missing journalctl | unavailable collector, other observations saved | Empty log proves no errors |
| Hanging child / excessive output | bounded return; timeout/non-OK; no live descendant | Unfinished source is complete |
| Store lock + SIGTERM | concurrent operation rejected; exit130; no saved incomplete new ID | Abrupt host power loss recovery proven |
| Corrupt SQLite/future schema | explicit failure; no fabricated valid snapshot | All migration versions are supported |
| 8 MiB guest-only tmpfs full | ENOSPC explicit, recovery after freeing own filler | Host disk full or arbitrary crash recovery tested |
| Retention dry run + pins | dry-run unchanged; pins survive gc/delete protection | Logical JSON quota is physical disk quota |
| Export synthetic identities | fixed 3 regular members; hashes; redaction; no boot/cursor; no overwrite/symlink/traversal | Absolute anonymity or arbitrary untrusted archive import safety |

Every scenario records PASS/FAIL, errors with traceback, and every CLI command's
exact args/exit/stdout/stderr. Evidence references in changes resolve to the actual
saved snapshot ID/collector/record. Findings require evidence, alternatives,
limitations and confidence basis. No assertion is removed to turn a failure green.

Unit tests cover non-UTF8 parsers, duplicate device identities and renumbering,
unknown snapshot schema, SQLite transactional interruption and output escaping.
These remain fixture-level evidence, not physical hardware evidence.
