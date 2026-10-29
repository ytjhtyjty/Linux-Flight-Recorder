#!/usr/bin/env python3

import json,pathlib,subprocess,shutil
m=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1','--filter-platform','x86_64-unknown-linux-gnu']))
out=pathlib.Path('licenses');out.mkdir(exist_ok=True)
index=['# Third-party notices','', 'Original legal text from registry source packages resolved for x86_64-unknown-linux-gnu,', 'including build/dev dependencies (a superset of binary runtime dependencies).', 'The full target-independent metadata SBOM also lists platforms not built here.', 'No assertion is made that this substitutes for maintainer legal review.', '', '| Package | Declared expression | Legal text directory |','|---|---|---|']
count=0
for p in sorted(m['packages'],key=lambda p:(p['name'],p['version'])):
    if not p.get('source'):continue
    root=pathlib.Path(p['manifest_path']).parent
    paths=[f for f in root.iterdir() if f.is_file() and any(f.name.lower().startswith(x) for x in ['license','copying','copyright','notice','unlicense','authors'])]
    for sub in ['licenses','LICENSES','license']:
        d=root/sub
        if d.is_dir():paths += [f for f in d.rglob('*') if f.is_file() and f.stat().st_size<262144]
    assert paths, 'Missing license files: '+p['name']
    dest=out/(p['name']+'-'+p['version']);dest.mkdir(exist_ok=True)
    for f in paths:
        rel=f.relative_to(root);to=dest/rel;to.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(f,to)
    if p['name']=='libsqlite3-sys':
        text=(root/'sqlite3/sqlite3.c').read_text();i=text.index('** The author disclaims');end=text.index('*************************************************************************',i)
        (dest/'SQLITE-PUBLIC-DOMAIN.txt').write_text(text[i:end]+'\nSource: bundled sqlite3/sqlite3.c public-domain notice.\n')
    index.append(f"| {p['name']} {p['version']} | {p.get('license','NOASSERTION')} | [{dest.as_posix()}]({dest.as_posix()}) |")
    count+=1
pathlib.Path('THIRD_PARTY_NOTICES.md').write_text('\n'.join(index)+'\n')
print(f'Copied original notice files for {count} dependency packages (Linux build/dev/runtime superset)')
