#!/usr/bin/env python3
import json,pathlib,re
root=pathlib.Path(__file__).resolve().parents[1]
for name in ['README.md','INSTALL.md','CONFIGURATION.md','PRIVACY.md','SECURITY.md','CONTRIBUTING.md','CHANGELOG.md','LICENSE','THIRD_PARTY_NOTICES.md','docs/ARCHITECTURE.md','docs/JSON_CONTRACT.md','docs/THREAT_MODEL.md','docs/RELEASE.md','docs/validation.md']:
    p=root/name;assert p.is_file() and p.stat().st_size>20,name
for p in root.glob('*.md'):
    for target in re.findall(r'\]\(([^)]+)\)',p.read_text()):
        if '://' not in target:assert (p.parent/target.split('#')[0]).exists(),(p,target)
json.load(open(root/'examples/lfr.json'));assert len((root/'man/lfr.1').read_text())>500
print('Documentation files, local top-level links and JSON example: OK')
