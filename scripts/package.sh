#!/bin/sh
set -eu
cargo build --release --locked
python3 scripts/notices.py
mkdir -p dist
version=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')
name="linux-flight-recorder-${version}-$(uname -m)-linux"
stage="dist/$name"
[ ! -e "$stage" ] || { echo "staging directory exists" >&2; exit 1; }
mkdir "$stage"
install -m755 target/release/lfr "$stage/lfr"
cp LICENSE README.md PRIVACY.md INSTALL.md CONFIGURATION.md SECURITY.md CONTRIBUTING.md CHANGELOG.md "$stage/"
cp -R docs man completions systemd examples licenses "$stage/"
cp THIRD_PARTY_NOTICES.md "$stage/"
python3 scripts/sbom.py "$stage/sbom.spdx.json"
tar -czf "dist/$name.tar.gz" -C dist "$name"
(cd dist && sha256sum "$name.tar.gz" > SHA256SUMS && sha256sum -c SHA256SUMS)
echo 'Candidate only: no signature or provenance attestation is configured.'
