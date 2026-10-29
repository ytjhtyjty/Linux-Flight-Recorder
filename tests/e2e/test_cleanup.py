#!/usr/bin/env python3




import json
import os
import pathlib
import signal
import subprocess
import sys
import time


def main():
    root=pathlib.Path(__file__).resolve().parents[2]
    work=pathlib.Path(sys.argv[1]).resolve()
    before=set(work.glob('run-*'))
    log=work/'cleanup-test.log'
    qpid=None
    run=None
    with log.open('wb') as out:
        p=subprocess.Popen([sys.executable,str(root/'tests/vm/run.py'),'--workdir',str(work),'--boot-only'],stdout=out,stderr=subprocess.STDOUT)
        try:
            deadline=time.monotonic()+90
            while time.monotonic()<deadline:
                assert p.poll() is None,'runner failed before QEMU startup'
                new=set(work.glob('run-*'))-before
                if new:
                    assert len(new)==1,'do not run cleanup test concurrently in same workdir'
                    run=new.pop()
                    children=pathlib.Path(f'/proc/{p.pid}/task/{p.pid}/children')
                    for child in children.read_text().split():
                        path=pathlib.Path(f'/proc/{child}/cmdline')
                        if path.exists() and b'qemu-system-x86_64' in path.read_bytes():qpid=int(child)
                    if qpid:break
                time.sleep(.2)
            assert qpid and run,'QEMU startup deadline'
            time.sleep(1)
            p.send_signal(signal.SIGTERM)
            code=p.wait(timeout=85)
            assert code!=0,'intentional interruption must not look successful'
            result=json.loads((run/'result.json').read_text())
            assert result['status']=='FAIL' and 'signal 15' in result['error'],result
            assert not pathlib.Path(f'/proc/{qpid}').exists(),'QEMU child remained'
            for name in ['ssh-key','ssh-key.pub','user-data','seed.iso']:assert not (run/name).exists(),name
            result={'status':'PASS','scenario':'SIGTERM during real QEMU boot','runner_exit':code,'qemu_pid':qpid,'run':run.name,'private_keys_removed':True,'qemu_exited':True}
            (work/'cleanup-result.json').write_text(json.dumps(result,indent=2)+'\n')
            print(json.dumps(result,indent=2))
        finally:
            if p.poll() is None:
                p.terminate()
                try:p.wait(timeout=85)
                except subprocess.TimeoutExpired:p.kill();p.wait(timeout=5)

            if qpid and run:
                path=pathlib.Path(f'/proc/{qpid}/cmdline')
                if path.exists() and str(run).encode() in path.read_bytes() and path.stat().st_uid==os.getuid():
                    os.killpg(qpid,signal.SIGTERM)

if __name__=='__main__':main()
