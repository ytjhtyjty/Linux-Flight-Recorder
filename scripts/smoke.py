#!/usr/bin/env python3

import json, os, pathlib, resource, subprocess, sys, tempfile, time
binary = os.path.realpath(sys.argv[1] if len(sys.argv)>1 else 'target/release/lfr')
measurements=[]
with tempfile.TemporaryDirectory(prefix='lfr-smoke-') as raw:
    store=os.path.realpath(raw)
    def run(*args, partial=False):
        start=time.monotonic()
        p=subprocess.run([binary,'--store',store,*args],capture_output=True,timeout=90)
        elapsed=time.monotonic()-start
        assert p.returncode in ([0,3] if partial else [0]), (args,p.returncode,p.stderr.decode(errors='replace'))
        assert b'\x1b' not in p.stdout+p.stderr
        measurements.append({'command':' '.join(args),'wall_seconds':round(elapsed,6),'child_peak_rss_kib_high_water':resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,'exit':p.returncode})
        return p.stdout
    run('--help');run('doctor');run('config','validate')
    ids=[json.loads(run('snapshot','create','--label',label,partial=True))['id'] for label in ['before-smoke','after-smoke']]
    for id_ in ids: assert json.loads(run('snapshot','show',id_))['id']==id_
    listing=json.loads(run('snapshot','list'));assert len(listing)==2
    comparison=json.loads(run('compare',*ids,'--format','json'));assert comparison['schema_version']==1
    run('compare',*ids)
    report=os.path.join(store,'report.md');run('report',*ids,'--symptom','wifi','--output',report)
    preview=json.loads(run('export',ids[1],'--preview'));assert len(preview['files'])==3
    bundle=os.path.join(store,'bundle.tar.gz');run('export',ids[1],'--output',bundle)
    import tarfile,hashlib
    with tarfile.open(bundle) as archive:
        assert archive.getnames()==preview['files']
        manifest=json.load(archive.extractfile('manifest.json'))
        for member in manifest['files']:
            data=archive.extractfile(member['path']).read();assert hashlib.sha256(data).hexdigest()==member['sha256']
    snapshots_public={}
    for label,id_ in zip(['before','after'],ids):
        path=os.path.join(store,label+'.tar.gz');run('export',id_,'--output',path)
        with tarfile.open(path) as archive:snapshots_public[label]=archive.extractfile('snapshot.json').read()
    run('snapshot','pin',ids[0]);run('gc','--dry-run');run('gc')
    run('snapshot','delete',ids[1]);run('snapshot','delete',ids[0],'--allow-pinned');assert json.loads(run('snapshot','list'))==[]
    collector_summary={label:{name:{'status':data['status'],'records':len(data['records']),'duration_ms':data['duration_ms'],'retained_records_json_bytes':len(json.dumps(data['records']).encode())} for name,data in json.loads(raw)['collectors'].items()} for label,raw in snapshots_public.items()}
    result={'collector_summary':collector_summary,'host_os_release':pathlib.Path('/etc/os-release').read_text(),'architecture':os.uname().machine,'kernel_release':os.uname().release,'binary':os.path.basename(binary),'measurement_method':'release build; wall monotonic; Linux child ru_maxrss KiB high-water, cumulative across commands; not per-collector peak','snapshots':listing,'measurements':measurements,'scope':'real accessible container sources; no updates, reboot, root, systemd or hardware validation'}
    out=pathlib.Path('examples/real-container');out.mkdir(parents=True,exist_ok=True)
    for label,data in snapshots_public.items():(out/(label+'.snapshot.json')).write_bytes(data)
    (out/'report.md').write_bytes(pathlib.Path(report).read_bytes())
    (out/'comparison.json').write_text(json.dumps(comparison,indent=2)+'\n')
    (out/'measurements.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'ok':True,'snapshots':[x['id'] for x in listing],'sizes':[x['bytes'] for x in listing],'measurements_saved':'examples/real-container/measurements.json'},indent=2))
