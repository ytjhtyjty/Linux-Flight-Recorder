# Contributing

Linux Flight Recorder is under development and may contain bugs. Development
contributions and reproducible nonsensitive bug reports are welcome; stable support
and hardware coverage are not promised.

Before opening a pull request, run formatting, clippy, Rust tests and the relevant
integration tests described in [validation](docs/validation.md). Add a regression
for a fix. Do not weaken assertions, hide failed checks or substitute mocks for
successful real-source tests. Keep code changes focused and preserve the privacy,
no-repair and no-causality-by-correlation boundaries.

Use synthetic data in fixtures and issues. Include OS/kernel/application versions,
exact commands, expected/actual results and access limitations. Review exports
before uploading them. Do not attach tokens, private keys or raw identifying logs.
Be respectful and discuss code and evidence rather than personal characteristics.

VM tests must stay rootless on the host and destructive only inside the disposable
guest. Do not introduce privileged containers, public SSH, physical passthrough,
untrusted-PR secrets or assumptions that hosted runners provide KVM.
