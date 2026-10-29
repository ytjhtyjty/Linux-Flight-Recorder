# Configuration and budgets

Configuration is explicit: `lfr --config /physical/path/lfr.json config validate`.
No implicit search or merging of root/user settings. All fields have defaults,
unknown fields fail, numbers outside supported ranges fail. See examples/lfr.json.
`config validate` prints the effective configuration as JSON, without opening storage.

| Setting | Default | Meaning |
|---|---:|---|
| command_timeout_ms | 5000 | Per external command, includes pipe draining |
| command_output_bytes | 4194304 | Combined stdout/stderr per command |
| source_bytes | 16384 | Per allowed sysfs/proc field |
| source_entries | 4096 | Directory entries per collector |
| source_timeout_ms | 3000 | Cooperative deadline between sysfs entries |
| journal_records | 200 | Maximum recent priority 0..3 system records |
| journal_window_seconds | 900 | Current-boot requested window ending at collection start |
| selected_services | NetworkManager, networkd, bluetooth | Up to 16 safe .service names; no user sessions |
| max_snapshots | 40 | Count target enforced by explicit gc |
| max_age_days | 30 | Age target enforced by explicit gc |
| max_store_bytes | 268435456 | Logical JSON quota checked before commit; gc target |

Each external process additionally has a 256 MiB virtual-address-space limit,
30 CPU-second limit and core dumps disabled (inherited by descendants). These are
not a bound on whole-tree RSS; wall-clock timeout kills the process group.
Sysfs collectors have an additional 2 MiB retained field-data budget; whole
snapshots are capped at 32 MiB serialized. A hard 1000-snapshot insertion cap
prevents unlimited index growth; run gc before reaching it. Maximum configuration caps cannot be
raised without code changes. SQLite page/rollback/VACUUM overhead and output files
are **not** included in the logical quota. During a transaction physical disk use
can exceed it: no hard total filesystem quota is promised. Use an OS quota or a
separate filesystem for a hard physical bound. A full disk fails without deleting
pinned records. `gc` vacuums to reclaim pages; allow temporary space for VACUUM.

User store: `$XDG_STATE_HOME/lfr` or `$HOME/.local/state/lfr`. System store:
`/var/lib/lfr`, explicit `--system`, root required. `--store` overrides location,
never privilege policy. Root without --system is rejected. Stores must use
absolute physical paths: no symlink components, owner uid, final mode 0700,
files 0600 with one hard link. Untrusted writable ancestors are rejected except
sticky directories. Config is a bounded JSON file; final symlinks are rejected.

`gc --dry-run` plans deletion, `gc` applies it. Pins override age/count/size. If
pins prevent limits, the JSON plan says `pinned_limit_exceeded`; snapshots are not
silently deleted. Save rejects a byte-quota violation; it does not silently run gc.
Timers run gc before snapshotting. Count/age are not automatically applied by
manual snapshot create; run gc regularly.

SIGINT/SIGTERM stops command collection and prevents a snapshot commit at the
precommit cancellation point. A signal racing with the SQLite commit may leave a
complete committed snapshot; there is no half-committed snapshot. Sysfs deadlines
are cooperative, not a hard guarantee for a pathological blocking kernel read.
