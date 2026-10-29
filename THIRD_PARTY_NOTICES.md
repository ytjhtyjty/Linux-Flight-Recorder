# Third-party dependencies and notices

The project source is Apache-2.0. Third-party dependencies keep their own licenses.
This source repository does not vendor dependency code or publish binary artifacts;
Cargo.lock pins the dependency resolution.

Before distributing a binary, run:

```sh
python3 scripts/notices.py
python3 scripts/sbom.py
```

The first script copies original dependency legal texts into `licenses/` and
replaces this file with a generated per-package index. The second produces SPDX
metadata. Review both outputs before distribution. `scripts/package.sh` performs
these generation steps before creating a candidate binary archive, including the
notice files. Generated inventories and license copies are not committed here.

The previous Linux build/dev dependency review covered 100 dependency packages;
target-independent Cargo metadata contained 132 packages including the project.
These counts are dated observations, not a legal guarantee or a substitute for
reviewing the exact dependencies and target used for a future release.
