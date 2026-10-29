#!/usr/bin/env python3

import argparse, atexit, hashlib, io, json, os, pathlib, resource, shutil, signal, socket, subprocess, sys, tarfile, time, urllib.request, uuid
from probe import ssh_ready, retryable_ssh_exit
REPO=pathlib.Path(__file__).resolve().parents[2]
assert os.geteuid()!=0, 'Run the host VM harness as a normal user, never root'
assert os.uname().machine=='x86_64', 'Pinned runtime/image require x86_64 host'
p=argparse.ArgumentParser();p.add_argument('--workdir',required=True);p.add_argument('--prepare-only',action='store_true');p.add_argument('--boot-only',action='store_true');p.add_argument('--binary',default=str(REPO/'target/release/lfr'));p.add_argument('--accel',choices=['tcg','kvm'],default='tcg');args=p.parse_args()
W=pathlib.Path(args.workdir).absolute();W.mkdir(mode=0o700,parents=True,exist_ok=True)
assert not W.is_symlink() and W.stat().st_uid==os.getuid() and W.stat().st_mode&0o077==0,'workdir must be private mode 0700'
R=W/'runtime';R.mkdir(exist_ok=True);P=R/'root';P.mkdir(exist_ok=True)
manifest=json.loads((REPO/'tests/vm/runtime-packages.lock.json').read_text());image=json.loads((REPO/'tests/vm/image.lock.json').read_text())
def download(url,dest,digest,algorithm):
    if not dest.exists():
        tmp=dest.with_suffix(dest.suffix+'.download')
        try:
            with urllib.request.urlopen(url,timeout=90) as src,tmp.open('wb') as out:shutil.copyfileobj(src,out)
            tmp.replace(dest)
        finally:
            if tmp.exists():tmp.unlink()
    h=hashlib.new(algorithm)
    with dest.open('rb') as f:
        while b:=f.read(1024*1024):h.update(b)
    assert h.hexdigest()==digest, f'checksum failed: {dest.name}'
for pkg in manifest:
    dest=R/pathlib.PurePosixPath(pkg['url']).name;download(pkg['url'],dest,pkg['sha256'],'sha256')
    if not (R/(pkg['package']+'.extracted')).exists():
        member=next(x for x in subprocess.check_output(['ar','t',str(dest)],text=True).splitlines() if x.startswith('data.tar'))
        b=subprocess.check_output(['ar','p',str(dest),member])
        if member.endswith('.zst'):b=subprocess.check_output(['zstd','-dc'],input=b)
        with tarfile.open(fileobj=io.BytesIO(b),mode='r:*') as tf:tf.extractall(P,filter='data')
        (R/(pkg['package']+'.extracted')).touch()
base=W/image['filename'];download(image['url'],base,image['sha512'],'sha512')
env=os.environ.copy();env['LD_LIBRARY_PATH']=f'{P}/usr/lib/x86_64-linux-gnu:{P}/lib/x86_64-linux-gnu';env['QEMU_MODULE_DIR']=str(P/'usr/lib/x86_64-linux-gnu/qemu')
Q=P/'usr/bin/qemu-system-x86_64';QI=P/'usr/bin/qemu-img';ISO=P/'usr/bin/genisoimage'
version=subprocess.check_output([str(Q),'--version'],env=env,text=True).splitlines()[0];print(version,flush=True)
if args.prepare_only:sys.exit(0)
run=W/('run-'+time.strftime('%Y%m%dT%H%M%S')+'-'+uuid.uuid4().hex[:6]);run.mkdir(mode=0o700)
(run/'run-marker').write_text('lfr-owned-disposable-run\n')
shutil.copyfile(__file__,run/'host-runner.py')
tested_binary=run/'lfr-under-test';shutil.copyfile(args.binary,tested_binary);tested_binary.chmod(0o755)
binary_sha256=hashlib.sha256(tested_binary.read_bytes()).hexdigest()
def remove_credentials():
    for name in ['ssh-key','ssh-key.pub','user-data','seed.iso']:
        f=run/name
        if f.exists():f.unlink()
atexit.register(remove_credentials)
key=run/'ssh-key';subprocess.run(['ssh-keygen','-q','-t','ed25519','-N','','-f',str(key)],check=True)
def interrupted(signum, frame):
    raise RuntimeError(f'VM harness received signal {signum}')
signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)
nonce=uuid.uuid4().hex
user_data='''#cloud-config
hostname: lfr-synth-host
manage_etc_hosts: true
package_update: false
package_upgrade: false
ssh_pwauth: false
disable_root: true
users:
  - name: lfrtest
    shell: /bin/bash
    groups: [sudo]
    sudo: "ALL=(ALL) NOPASSWD:ALL"
    lock_passwd: true
    ssh_authorized_keys:
      - '''+key.with_suffix('.pub').read_text().strip()+'''
write_files:
  - path: /etc/lfr-validation-guest
    permissions: '0600'
    content: '''+nonce+'''\n'''
(run/'user-data').write_text(user_data);(run/'meta-data').write_text(f'instance-id: lfr-{nonce}\nlocal-hostname: lfr-synth-host\n')
seed=run/'seed.iso';subprocess.run([str(ISO),'-quiet','-output',str(seed),'-volid','cidata','-joliet','-rock',str(run/'user-data'),str(run/'meta-data')],env=env,check=True)
overlay=run/'overlay.qcow2';subprocess.run([str(QI),'create','-f','qcow2','-F','qcow2','-b',str(base),str(overlay)],env=env,check=True,stdout=subprocess.DEVNULL)
subprocess.run([str(QI),'resize',str(overlay),'6G'],env=env,check=True,stdout=subprocess.DEVNULL)
with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
sshbase=['ssh','-i',str(key),'-p',str(port),'-o','BatchMode=yes','-o','ConnectTimeout=3','-o','LogLevel=ERROR','-o','StrictHostKeyChecking=accept-new','-o',f'UserKnownHostsFile={run}/known_hosts','lfrtest@127.0.0.1']
commands=[]
def remote(command,timeout=180,check=True):
    t=time.monotonic();result=subprocess.run(sshbase+[command],capture_output=True,timeout=timeout)
    n=len(commands);(run/f'ssh-{n:03d}.stdout').write_bytes(result.stdout);(run/f'ssh-{n:03d}.stderr').write_bytes(result.stderr)
    commands.append({'command':command,'exit':result.returncode,'seconds':round(time.monotonic()-t,3),'stdout':f'ssh-{n:03d}.stdout','stderr':f'ssh-{n:03d}.stderr'});(run/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
    if check and result.returncode:raise RuntimeError(f'guest command exit {result.returncode}; inspect ssh-{n:03d}.stderr')
    return result
proc=None
log=None
def limits():
    resource.setrlimit(resource.RLIMIT_CORE,(0,0));resource.setrlimit(resource.RLIMIT_AS,(3*1024**3,3*1024**3));resource.setrlimit(resource.RLIMIT_FSIZE,(8*1024**3,8*1024**3));resource.setrlimit(resource.RLIMIT_CPU,(1200,1200))
def start(extra=False):
    global proc,log
    idx=1 if extra else 0;log=(run/f'qemu-{idx}.log').open('wb')
    cmd=[str(Q),'-accel',args.accel+',thread=multi' if args.accel=='tcg' else 'kvm','-cpu','max','-machine','q35','-smp','2','-m','1024','-nodefaults','-display','none','-monitor','none','-no-reboot','-serial',f'file:{run}/serial-{idx}.log','-bios',str(P/'usr/share/seabios/bios-256k.bin'),'-L',str(P/'usr/share/qemu'),'-drive',f'file={overlay},format=qcow2,if=virtio','-drive',f'file={seed},format=raw,if=virtio,readonly=on','-netdev',f'user,id=n0,hostfwd=tcp:127.0.0.1:{port}-:22','-device','virtio-net-pci,netdev=n0,addr=0x3,romfile=','-device','virtio-rng-pci']
    if extra:cmd+=['-netdev','user,id=n1','-device','e1000,netdev=n1,addr=0x6,romfile=']
    (run/f'qemu-command-{idx}.json').write_text(json.dumps(cmd,indent=2)+'\n')
    proc=subprocess.Popen(cmd,env=env,stdout=log,stderr=log,start_new_session=True,preexec_fn=limits)
    end=time.monotonic()+300
    while time.monotonic()<end:
        if proc.poll() is not None:raise RuntimeError('QEMU exited; inspect qemu log')
        if ssh_ready(sshbase+['true']):
            try:
                result=remote('sudo -n cloud-init status --wait',timeout=max(1,min(180,end-time.monotonic())),check=False)
            except subprocess.TimeoutExpired:
                continue


            if retryable_ssh_exit(result.returncode):time.sleep(2);continue
            if result.returncode:raise RuntimeError('cloud-init provisioning failed; inspect SSH logs')
            result=remote(f'test "$(sudo -n cat /etc/lfr-validation-guest)" = {nonce} && test "$(cat /proc/1/comm)" = systemd',check=False)
            if retryable_ssh_exit(result.returncode):time.sleep(2);continue
            if result.returncode:raise RuntimeError('guest nonce/systemd safety gate failed')
            print(f'VM boot {idx}: SSH and cloud-init ready',flush=True);return
        time.sleep(2)
    raise TimeoutError('VM SSH readiness timeout')
def stop():
    global proc,log
    if proc is not None and proc.poll() is None:
        try:remote('sudo -n systemctl poweroff',timeout=15,check=False)
        except Exception:pass
        try:proc.wait(timeout=40)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid,signal.SIGTERM)
            try:proc.wait(timeout=8)
            except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=8)
    if log:log.close()
def collect():
    try:
        result=remote('sudo -n tar -czf - -C /var/tmp lfr-evidence',timeout=90,check=False)
        if result.returncode==0:(run/'guest-evidence.tar.gz').write_bytes(result.stdout)
    except Exception as e:(run/'artifact-error.txt').write_text(str(e))
try:
    start()
    info=remote('uname -a; cat /etc/os-release; systemctl --version | head -1; id; ls /sys/bus/pci/devices; systemctl is-system-running || true')
    (run/'guest-info.txt').write_bytes(info.stdout)
    (W/'latest-run.txt').write_text(str(run)+'\n')
    if args.boot_only:
        print(info.stdout.decode(errors='replace'),flush=True)
    else:
        payload=run/'payload.tar'
        with tarfile.open(payload,'w') as t:
            t.add(tested_binary,arcname='lfr');t.add(REPO/'tests/vm/guest.py',arcname='guest.py')
            t.add(REPO/'systemd',arcname='systemd');t.add(REPO/'man',arcname='man');t.add(REPO/'completions',arcname='completions')
        with payload.open('rb') as f:r=subprocess.run(sshbase+['mkdir -p /home/lfrtest/payload && tar -xf - -C /home/lfrtest/payload'],stdin=f,capture_output=True,timeout=60)
        assert r.returncode==0,'payload transfer failed'
        phase_before=remote(f'sudo -n python3 /home/lfrtest/payload/guest.py --confirm-disposable {nonce} --phase before',timeout=600,check=False)
        collect();stop();start(extra=True)
        phase_after=remote(f'sudo -n python3 /home/lfrtest/payload/guest.py --confirm-disposable {nonce} --phase after',timeout=600,check=False)
        collect()
        assert phase_before.returncode==phase_after.returncode==0,'Guest assertions failed; inspect guest-evidence/results.json and command logs'
    (run/'result.json').write_text(json.dumps({'status':'PASS','qemu':version,'accel':args.accel,'image':image,'binary_sha256':binary_sha256},indent=2)+'\n')
    print('Artifacts:',run,flush=True)
except Exception as e:
    collect() if proc is not None and proc.poll() is None else None
    (run/'result.json').write_text(json.dumps({'status':'FAIL','error':str(e)},indent=2)+'\n');print('FAILED:',e,'Artifacts:',run,flush=True);raise
finally:
    stop()

    for f in [key,key.with_suffix('.pub'),run/'user-data',seed]:
        if f.exists():f.unlink()
