# Optional systemd automation — experimental; Debian 12 VM boot path tested

No continuously running daemon. Boot collection is deferred by a timer, not put
in the boot critical path. All operations share a private-store flock; overlap
fails clearly rather than running parallel full collections. Timers run retention
before capture; partial saved snapshots (exit 3) are accepted. Journal persistence
and source access remain explicit limitations.

The boot service hardening was runtime-tested in the pinned Debian 12 TCG VM;
see [validation](../docs/validation.md). All four units passed systemd-analyze
verify; boot service/timer and exit3 acceptance ran. The daily timer was not
waited for, and this is not proof on other distros or physical hardware.
PrivateDevices avoids real device I/O; collection uses sysfs. ProtectKernelLogs
protects direct kernel-log interfaces; journalctl reads journal files. ProtectHome
prevents arbitrary user-session access. Read-only /sys and /proc metadata must still
be checked on each real target. No network address families beyond AF_UNIX needed.
StateDirectory is the only persistent write exception under ProtectSystem=strict.
Resource limits can terminate pathological collection; no boot/reboot dependencies
require these services to succeed.

Administrator-only optional installation (executed only inside the disposable
validation VM, never on the host):

```sh
systemd-analyze verify systemd/lfr-boot.service systemd/lfr-periodic.service systemd/lfr-boot.timer systemd/lfr-periodic.timer
install -m644 systemd/lfr-*.service systemd/lfr-*.timer /etc/systemd/system/
systemctl daemon-reload
systemctl start lfr-boot.service
journalctl -u lfr-boot.service --no-pager -n 30
# Only after successful target validation and explicit approval:
systemctl enable --now lfr-boot.timer lfr-periodic.timer
```

No package hook is shipped. Package-event is a provenance label for an explicitly
invoked capture, not a promise of verified transaction automation. A manual baseline
or last periodic snapshot is not a guaranteed pre-transaction snapshot. A capture
after package installation but before reboot still observes the old running kernel.
