#!/usr/bin/env python3

import hashlib,json,subprocess,pathlib,sys,os,time,datetime
m=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1']))
packages=[]
for p in sorted(m['packages'],key=lambda p:(p['name'],p['version'])):
    license_=(p.get('license') or 'NOASSERTION').replace('/', ' OR ')
    packages.append({'name':p['name'],'SPDXID':'SPDXRef-'+p['name']+'-'+p['version'],'versionInfo':p['version'],'downloadLocation':'https://crates.io/crates/'+p['name']+'/'+p['version'] if p.get('source') else 'NOASSERTION','filesAnalyzed':False,'licenseConcluded':'NOASSERTION','licenseDeclared':license_,'copyrightText':'NOASSERTION'})
lock=pathlib.Path('Cargo.lock').read_bytes(); digest=hashlib.sha256(lock).hexdigest()

doc={'spdxVersion':'SPDX-2.3','dataLicense':'CC0-1.0','SPDXID':'SPDXRef-DOCUMENT','name':'linux-flight-recorder-locked-dependency-inventory','documentNamespace':'https://spdx.org/spdxdocs/lfr-'+digest,'creationInfo':{'creators':['Tool: lfr-sbom.py'],'created':datetime.datetime.fromtimestamp(int(os.environ.get('SOURCE_DATE_EPOCH',time.time())),datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')},'packages':packages,'documentDescribes':[p['SPDXID'] for p in packages if p['name']=='linux-flight-recorder']}
path=pathlib.Path(sys.argv[1] if len(sys.argv)>1 else 'docs/sbom.spdx.json');path.write_text(json.dumps(doc,indent=2)+'\n')
print(f'{len(packages)} package metadata entries -> {path}')
