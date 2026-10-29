#!/usr/bin/env python3




import argparse, contextlib, hashlib, json, os, pathlib, pwd, shutil, signal, sqlite3, subprocess, sys, tarfile, time, traceback
p=argparse.ArgumentParser();p.add_argument('--confirm-disposable',required=True);p.add_argument('--phase',choices=['before','after'],required=True);a=p.parse_args()
assert os.geteuid()==0 and pathlib.Path('/etc/lfr-validation-guest').read_text().strip()==a.confirm_disposable
assert pathlib.Path('/proc/1/comm').read_text().strip()=='systemd' and os.uname().nodename=='lfr-synth-host'
E=pathlib.Path('/var/tmp/lfr-evidence');E.mkdir(mode=0o700,exist_ok=True)
P=pathlib.Path(__file__).resolve().parent; B='/usr/local/bin/lfr';cfg=E/'config.json';statefile=E/'state.json'
results=json.loads((E/'results.json').read_text()) if (E/'results.json').exists() else []
commands=json.loads((E/'commands.json').read_text()) if (E/'commands.json').exists() else []
def save():
    (E/'results.json').write_text(json.dumps(results,indent=2)+'\n');(E/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
def cmd(argv,ok=(0,),timeout=90,env=None):
    t=time.monotonic();r=subprocess.run(list(map(str,argv)),capture_output=True,timeout=timeout,env=env)
    i=len(commands);out=f'command-{i:03d}.stdout';err=f'command-{i:03d}.stderr';(E/out).write_bytes(r.stdout);(E/err).write_bytes(r.stderr)
    commands.append(dict(argv=list(map(str,argv)),exit=r.returncode,seconds=round(time.monotonic()-t,3),stdout=out,stderr=err));save()
    assert r.returncode in ok,(argv,r.returncode,r.stderr.decode(errors='replace')[:1500]);return r

def cli(*argv,user=False,config=True,store=None,ok=(0,)):
    xs=[B]
    if config:xs+=['--config',str(cfg)]
    if not user:xs+=['--system']
    if store:xs+=['--store',str(store)]
    xs+=list(argv)
    if user:xs=['runuser','-u','lfrtest','--',*xs]
    deadline=time.monotonic()+60
    while True:
        r=cmd(xs,ok=tuple(set(ok)|{1}))
        if r.returncode in ok:return r


        if b'store busy: another lfr operation holds the lock' in r.stderr and time.monotonic()<deadline:
            time.sleep(1);continue
        raise AssertionError((xs,r.returncode,r.stderr.decode(errors='replace')[:1500]))
def data(*args,**kwargs):return json.loads(cli(*args,**kwargs).stdout)
def scenario(name,kind='VM'):
    def wrap(f):
        try:
            detail=f();results.append(dict(name=name,layer=kind,status='PASS',detail=detail))
        except Exception as ex:
            results.append(dict(name=name,layer=kind,status='FAIL',error=str(ex),traceback=traceback.format_exc()));print('FAIL',name,str(ex),flush=True)
        save();return f
    return wrap

def snap(label,user=False,config=True,display_label=None):
    v=data('snapshot','create','--label',display_label if display_label is not None else label,user=user,config=config,ok=(0,3));s=data('snapshot','show',v['id'],user=user,config=config)
    assert s['id']==v['id'] and s['finished_at']>=s['started_at']
    (E/(label+'.snapshot.json')).write_text(json.dumps(s,indent=2)+'\n');return s

def compare(x,y,label):
    d=data('compare',x['id'],y['id'],'--format','json');assert d['before_id']==x['id'] and d['after_id']==y['id']
    cli('compare',x['id'],y['id']);(E/(label+'.comparison.json')).write_text(json.dumps(d,indent=2)+'\n')
    for c in d['changes']:
        for ref in c['evidence']:

            assert ref.startswith('snapshot:'),ref
            parts=ref[len('snapshot:'):].split('/')
            ss={x['id']:x,y['id']:y}[parts[0]];assert parts[1]=='collectors' and parts[2] in ss['collectors']
            if len(parts)>3:assert parts[3]=='records' and 0<=int(parts[4])<len(ss['collectors'][parts[2]]['records'])
    for f in d['findings']:assert f['evidence'] and f['alternatives'] and f['limitations'] and f['confidence_basis']
    return d

def package(version):
    root=E/('deb-'+version);(root/'DEBIAN').mkdir(parents=True,exist_ok=True);(root/'usr/share/lfr-e2e-probe').mkdir(parents=True,exist_ok=True)
    (root/'DEBIAN/control').write_text(f'Package: lfr-e2e-probe\nVersion: {version}\nArchitecture: all\nMaintainer: Synthetic Test <test@example.invalid>\nDescription: disposable LFR validation data\n')
    (root/'usr/share/lfr-e2e-probe/version').write_text(version+'\n');out=E/('probe-'+version+'.deb');cmd(['dpkg-deb','--build','--root-owner-group',root,out]);cmd(['dpkg','-i',out])
def pkgchange(d,kind):return [c for c in d['changes'] if c['collector']=='packages' and c['kind']==kind and 'name=lfr-e2e-probe;' in c['identity']]
@contextlib.contextmanager
def journal_override(body=None,missing=False):
    original=pathlib.Path('/usr/bin/journalctl');backup=pathlib.Path('/usr/bin/journalctl.lfr-validation-backup');assert not backup.exists();original.rename(backup)
    try:
        if not missing:original.write_text('#!/usr/bin/python3\n'+body);original.chmod(0o755)
        yield
    finally:
        if original.exists():original.unlink()
        backup.rename(original)

if a.phase=='before':
    assert not pathlib.Path(B).exists(),'refuse to overwrite preinstalled binary'
    shutil.copyfile(P/'lfr',B);os.chmod(B,0o755)

    cmd(['systemctl','mask','--now','apt-daily.timer','apt-daily-upgrade.timer','apt-daily.service','apt-daily-upgrade.service'])
    cfg.write_text(json.dumps({'selected_services':['lfr-e2e.service']}))
    state={}
    @scenario('installation-help-version-doctor-config')
    def _():
        cli('--help');assert '0.1.0' in cli('--version').stdout.decode();d=data('doctor');assert d['systemd_runtime'] and d['uid']==0;assert data('config','validate')['valid']
        for src,dst in [(P/'man/lfr.1',pathlib.Path('/usr/local/share/man/man1/lfr.1'))]:
            if src.exists():dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dst)
        completion=pathlib.Path('/usr/local/share/bash-completion/completions/lfr');completion.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(P/'completions/lfr.bash',completion)
        cmd(['file',B],ok=(0,127)) if shutil.which('file') else None
        (E/'environment.txt').write_bytes(cmd(['uname','-a']).stdout+cmd(['systemctl','--version']).stdout+pathlib.Path('/etc/os-release').read_bytes())
    unit=pathlib.Path('/etc/systemd/system/lfr-e2e.service')
    unit.write_text('[Unit]\nDescription=Synthetic LFR validation only\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/usr/bin/true\n')
    cmd(['systemctl','daemon-reload']);cmd(['systemctl','start','lfr-e2e.service'])
    baseline=snap('baseline');state['baseline']=baseline['id'];state['boot_id']=baseline['boot_id']
    @scenario('root-real-collectors-and-private-storage')
    def _():
        c=baseline['collectors'];assert c['packages']['status']=='ok' and len(c['packages']['records'])>100,c['packages']
        for name in ['pci','network','services','selected_services','system']:assert c[name]['status']=='ok',(name,c[name])
        assert len(c['pci']['records'])>3 and c['journal']['status']!='ok' and baseline['completeness']!='ok'
        store=pathlib.Path('/var/lib/lfr');assert store.stat().st_mode&0o777==0o700
        for f in store.iterdir():assert f.stat().st_mode&0o077==0,(str(f),oct(f.stat().st_mode))
        cli('snapshot','list',user=True,config=False,store=store,ok=(1,));return {n:{'status':v['status'],'records':len(v['records'])} for n,v in c.items()}
    @scenario('normal-user-permission-limited-snapshot')
    def _():
        s=snap('unprivileged',user=True,config=False);assert s['privilege']!='root';assert s['collectors']['journal']['status']=='permission_denied';state['user_before']=s['id']
    @scenario('packages-added-real-dpkg')
    def _():
        package('1.0');s=snap('package-added');d=compare(baseline,s,'package-added');assert pkgchange(d,'added');state['package1']=s['id']
    @scenario('packages-version-changed-real-dpkg')
    def _():
        x=data('snapshot','show',state['package1']);package('2.0');s=snap('package-upgraded');d=compare(x,s,'package-upgraded');cs=pkgchange(d,'changed');assert len(cs)==1 and cs[0]['before']['version']=='1.0' and cs[0]['after']['version']=='2.0'
        state['package2']=s['id']
    @scenario('packages-removed-real-dpkg')
    def _():
        x=data('snapshot','show',state['package2']);cmd(['dpkg','--purge','lfr-e2e-probe']);s=snap('package-removed');assert pkgchange(compare(x,s,'package-removed'),'removed')
    unit.write_text(unit.read_text().replace('/usr/bin/true','/usr/bin/false'));cmd(['systemctl','daemon-reload']);cmd(['systemctl','restart','lfr-e2e.service'],ok=(1,))
    marker='LFR_SYNTHETIC_JOURNAL_'+a.confirm_disposable[:12];state['journal_marker']=marker
    msg=marker+' host=lfr-synth-host user=lfrtest collector_user=root ip=192.0.2.123 mac=02:42:ac:11:00:09 home=/home/lfrtest/private api_key=SYNTHETIC_LFR_TOKEN_0123456789 <script> [click](https://example.invalid) \x1b[31mRED\x1b[0m \u202e'


    plain=msg.split(' <script>')[0]
    cmd(['logger','-p','user.err','-t','lfr-e2e','--',plain]);cmd(['logger','-p','user.err','-t','lfr-e2e-binary','--',msg]);cmd(['journalctl','--sync']);time.sleep(1.2)
    changed=snap('service-and-journal',display_label='<script>[click](bad) \x1b[31mRED\x1b[0m \u202e');state['changed']=changed['id']
    @scenario('systemd-healthy-to-failed-observation-and-evidence')
    def _():
        d=compare(baseline,changed,'service-and-journal');assert any(c['collector']=='services' and c['kind']=='added' and c['after']['name']=='lfr-e2e.service' for c in d['changes']);assert any(f['rule_id']=='LFR-SERVICE-001' for f in d['findings'])
        assert any(r.get('active')=='failed' for r in changed['collectors']['selected_services']['records']),changed['collectors']['selected_services']
    @scenario('journal-real-window-and-no-unjustified-novelty')
    def _():
        j=changed['collectors']['journal'];assert any(marker in r.get('message',r.get('MESSAGE','')) for r in j['records']),j
        assert any(r.get('message')=='[non-scalar journal field omitted]' for r in j['records'])
        d=compare(baseline,changed,'journal-coverage');assert all(c['kind']=='not_comparable' for c in d['changes'] if c['collector']=='journal')
        return {'journal_status':j['status'],'limitations':j['limitations']}
    @scenario('report-export-preview-redaction-terminal-markdown')
    def _():
        report=E/'report.md';cli('report',baseline['id'],changed['id'],'--symptom','wifi','--output',report)
        md=report.read_bytes();assert b'\\u{3c}script' in md and b'\\u{1b}' in md
        assert b'\x1b' not in md and b'\xe2\x80\xae' not in md and b'<script>' not in md
        preview=data('export',changed['id'],'--preview');archive=E/'public-export.tar.gz';cli('export',changed['id'],'--output',archive)
        with tarfile.open(archive) as t:
            assert t.getnames()==preview['files']==['snapshot.json','manifest.json','PRIVACY.txt'];assert all(m.isfile() and '/' not in m.name and '..' not in m.name for m in t.getmembers())
            public=t.extractfile('snapshot.json').read();s=json.loads(public);assert s['boot_id'] is None
            for x in json.load(t.extractfile('manifest.json'))['files']:assert hashlib.sha256(t.extractfile(x['path']).read()).hexdigest()==x['sha256']
            for secret in ['lfr-synth-host','192.0.2.123','02:42:ac:11:00:09','/home/lfrtest/private','SYNTHETIC_LFR_TOKEN_0123456789']:assert secret.encode() not in public,secret
            assert all('cursor' not in r and 'boot_id' not in r for v in s['collectors'].values() for r in v['records'])
            assert b'collector_user=root' not in public

            assert b'user=lfrtest' in public,'Re-evaluate the documented privacy boundary if masking policy changes'
        cli('export',changed['id'],'--output',archive,ok=(1,));link=E/'report-link.md';link.symlink_to(report);cli('report',baseline['id'],changed['id'],'--output',link,ok=(1,));assert report.read_bytes()==md
        cli('snapshot','show','../../etc/shadow',ok=(1,));return 'Known hostname/root user and IP/MAC/home/token samples masked; arbitrary other username user=lfrtest remains. No anonymity guarantee; manual review mandatory.'
    @scenario('same-user-journal-access-change-no-new-error-claim')
    def _():
        assert 'user_before' in state
        cmd(['usermod','-aG','systemd-journal','lfrtest'])
        later=snap('user-journal-access',user=True,config=False)
        d=data('compare',state['user_before'],later['id'],'--format','json',user=True,config=False)
        assert all(c['kind']=='not_comparable' for c in d['changes'] if c['collector']=='journal')
        assert any(marker in r.get('message','') for r in later['collectors']['journal']['records'])
        return {'journal_before':'permission-limited (see unprivileged.snapshot.json)','journal_after':later['collectors']['journal']['status']}
    @scenario('source-missing-snapshot-is-partial','VM fault injection')
    def _():
        with journal_override(missing=True):s=snap('journal-missing')
        assert s['collectors']['journal']['status']=='unavailable' and s['completeness']!='ok';d=compare(s,changed,'journal-missing-vs-readable');assert all(c['kind']=='not_comparable' for c in d['changes'] if c['collector']=='journal')
    @scenario('source-timeout-output-cap-and-descendant-cleanup','VM fault injection')
    def _():
        old=cfg.read_text();cfg.write_text(json.dumps({'command_timeout_ms':1500,'command_output_bytes':1024,'selected_services':['lfr-e2e.service']}))
        try:
            with journal_override('import os,time\npid=os.fork()\nif pid==0:\n open("/var/tmp/lfr-evidence/hanging-child.pid","w").write(str(os.getpid()))\n time.sleep(60)\nelse: time.sleep(60)\n'):
                s=snap('journal-timeout');assert s['collectors']['journal']['status']=='timeout',s['collectors']['journal'];time.sleep(.2)
                pid=int((E/'hanging-child.pid').read_text());proc=pathlib.Path(f'/proc/{pid}/stat');assert not proc.exists() or proc.read_text().split()[2]=='Z','live descendant remained'
            with journal_override('import os\nfor i in range(100000): os.write(1,b"x"*1024)\n'):
                s=snap('journal-capped');assert s['collectors']['journal']['status']!='ok' and s['collectors']['packages']['status']!='unavailable'
                assert s['collectors']['system']['status']=='ok' and s['collectors']['pci']['records']
            with journal_override('import os\nfor i in range(100000): os.write(2,b"y"*1024)\n'):
                s=snap('journal-stderr-capped');assert s['collectors']['journal']['status']!='ok' and s['collectors']['system']['status']=='ok'
        finally:cfg.write_text(old)
    @scenario('concurrency-lock-and-interrupted-collection','VM fault injection')
    def _():
        import fcntl
        lock=pathlib.Path('/var/lib/lfr/lock')
        assert lock, list(pathlib.Path('/var/lib/lfr').iterdir())
        with lock.open('r+') as f:
            fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB);cli('snapshot','list',ok=(1,))
        before=data('snapshot','list');out=E/'interrupted.stdout';err=E/'interrupted.stderr'
        with journal_override('import time\ntime.sleep(60)\n'),out.open('wb') as o,err.open('wb') as e:
            argv=[B,'--system','--config',str(cfg),'snapshot','create'];t=time.monotonic();child=subprocess.Popen(argv,stdout=o,stderr=e);time.sleep(1);child.send_signal(signal.SIGTERM);code=child.wait(timeout=15)
            commands.append(dict(argv=argv,exit=code,signal_sent='SIGTERM',seconds=round(time.monotonic()-t,3),stdout=out.name,stderr=err.name));save();assert code==130
        assert len(data('snapshot','list'))==len(before)
    @scenario('sqlite-migration-future-schema-corruption','VM fault injection')
    def _():
        parent=pathlib.Path('/var/lib/lfr-validation');parent.mkdir(mode=0o700,exist_ok=True)
        for n in ['future','corrupt','migration']:
            d=parent/n;cli('snapshot','list',store=d)
            db=next(d.glob('*.sqlite*'));con=sqlite3.connect(db)
            if n=='future':con.execute('PRAGMA user_version=99');con.close();cli('snapshot','list',store=d,ok=(1,))
            elif n=='migration':assert con.execute('PRAGMA user_version').fetchone()[0]==1;con.close()
            else:
                con.close();db.write_bytes(b'not a SQLite database');cli('snapshot','list',store=d,ok=(1,))
    @scenario('isolated-tmpfs-ENOSPC-and-recovery','VM fault injection')
    def _():
        d=pathlib.Path('/var/lib/lfr-full');d.mkdir(mode=0o700);cmd(['mount','-t','tmpfs','-o','size=8m,mode=0700','lfr-e2e-tmpfs',d])
        try:
            fill=d/'fill'
            try:
                with fill.open('wb') as f:
                    for i in range(32):f.write(b'x'*1024*1024)
            except OSError as e:assert e.errno==28
            cli('snapshot','create',store=d,ok=(1,));fill.unlink();assert data('snapshot','list',store=d)==[]
        finally:cmd(['umount',d]);d.rmdir()
    @scenario('retention-dry-run-pin-and-delete')
    def _():
        cli('snapshot','pin',baseline['id']);cli('snapshot','pin',changed['id']);old=cfg.read_text();cfg.write_text(json.dumps({'max_snapshots':2}))
        try:
            before=data('snapshot','list');plan=data('gc','--dry-run');assert data('snapshot','list')==before;data('gc');left=data('snapshot','list');assert {r['id'] for r in left}>={baseline['id'],changed['id']};cli('snapshot','delete',baseline['id'],ok=(1,));return {'before':len(before),'after':len(left),'plan':plan}
        finally:cfg.write_text(old)
    cmd(['systemctl','reset-failed','lfr-e2e.service'])
    @scenario('systemd-units-install-hardening-and-exit3')
    def _():
        for src in (P/'systemd').glob('lfr-*'):shutil.copyfile(src,pathlib.Path('/etc/systemd/system')/src.name)
        cmd(['systemd-analyze','verify',*map(str,(P/'systemd').glob('lfr-*'))]);cmd(['systemctl','daemon-reload']);cmd(['systemctl','enable','--now','lfr-boot.timer']);cmd(['systemctl','start','lfr-boot.service'],timeout=90)
        r=cmd(['systemctl','show','lfr-boot.service','-p','Result','-p','ExecMainStatus']);assert b'Result=success' in r.stdout and b'ExecMainStatus=3' in r.stdout
        cmd(['systemctl','enable','lfr-boot.timer']);return r.stdout.decode()
    statefile.write_text(json.dumps(state,indent=2)+'\n')
else:


    deadline=time.monotonic()+180
    while float(pathlib.Path('/proc/uptime').read_text().split()[0])<160 and time.monotonic()<deadline:time.sleep(3)
    while time.monotonic()<deadline:
        active=cmd(['systemctl','show','lfr-boot.service','-p','ActiveState','--value']).stdout.strip()
        if active not in [b'activating',b'active',b'deactivating']:break
        time.sleep(2)
    state=json.loads(statefile.read_text());baseline=data('snapshot','show',state['baseline']);after=snap('after-reboot')
    @scenario('real-reboot-new-bootid-persistence-own-kernel')
    def _():
        assert state['boot_id'] and after['boot_id']!=state['boot_id'];assert baseline['id']==state['baseline'];d=compare(baseline,after,'reboot-device');assert any('Boot IDs differ' in x for x in d['limitations']);return {'before_boot_id':state['boot_id'],'after_boot_id':after['boot_id'],'running_kernel':os.uname().release,'kernel_upgrade':'NOT RUN (same kernel booted)'}
    @scenario('virtual-e1000-added-without-management-renumbering')
    def _():
        d=compare(baseline,after,'virtual-device');cs=[c for c in d['changes'] if c['collector']=='pci' and c['kind']=='added' and c['after'].get('device')=='0x100e'];assert len(cs)==1,[(c['kind'],c.get('after')) for c in d['changes'] if c['collector']=='pci'];assert any(r.get('device')=='0x1000' or r.get('vendor')=='0x1af4' for r in after['collectors']['pci']['records']);return cs
    @scenario('boot-timer-created-snapshot-not-boot-critical')
    def _():
        deadline=time.monotonic()+220
        while time.monotonic()<deadline:
            found=[r for r in data('snapshot','list') if data('snapshot','show',r['id'])['source']=='boot' and data('snapshot','show',r['id'])['boot_id']==after['boot_id']]
            if found:break
            time.sleep(5)
        assert found,'boot timer did not save a snapshot';cmd(['systemctl','show','lfr-boot.service','-p','Result','-p','ExecMainStatus']);critical=cmd(['systemd-analyze','critical-chain','multi-user.target']);assert b'lfr-boot.service' not in critical.stdout;return found
    @scenario('documented-uninstall-guest-only')
    def _():
        cmd(['systemctl','disable','--now','lfr-boot.timer']);cmd(['systemctl','stop','lfr-e2e.service'],ok=(0,))
        for src in (P/'systemd').glob('lfr-*'):(pathlib.Path('/etc/systemd/system')/src.name).unlink()
        pathlib.Path('/etc/systemd/system/lfr-e2e.service').unlink();cmd(['systemctl','daemon-reload']);pathlib.Path(B).unlink()
        man=pathlib.Path('/usr/local/share/man/man1/lfr.1')
        if man.exists():man.unlink()
        pathlib.Path('/usr/local/share/bash-completion/completions/lfr').unlink()
        assert not pathlib.Path(B).exists();return 'Data retained deliberately; only installed test files removed.'
print(json.dumps({'phase':a.phase,'pass':sum(r['status']=='PASS' for r in results),'fail':sum(r['status']=='FAIL' for r in results)},indent=2),flush=True)
sys.exit(1 if any(r['status']=='FAIL' for r in results) else 0)
