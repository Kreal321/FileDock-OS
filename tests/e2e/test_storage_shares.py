"""US9 Linux acceptance. Destructive/fill checks refuse an existing PVE host.
Run on fresh libvirt VMs with a complete offline bundle, never on a user's installation.
"""
import json, shlex, time
import httpx
import pytest
from conftest import VmSpec, SMALL_DISK_VM, run
from state_helpers import setup, wait


def fresh(vm_factory,e2e_env,*,small=False,disks=5):
    if e2e_env.existing_host:
        pytest.skip('Storage/fill acceptance is restricted to newly provisioned disposable libvirt VMs.')
    vm=vm_factory(SMALL_DISK_VM if small else VmSpec(data_disks_gb=(20,)*disks))
    vm.install();return vm,setup(vm)


def operation(api,path,body):
    response=api.post(path,json=body);response.raise_for_status()
    result=api.wait_operation(response.json()['id']);assert result['state']=='Succeeded',result
    return result


def pool(api,name):
    return next(p for p in api.get('/pools').raise_for_status().json()['items'] if p['spec']['name']==name)


def folder(api,pool_id,name,access=None):
    response=api.post('/folders',json={'identity':{'name':name},'storageMode':'LocalOnly','reference':{'poolId':pool_id,'relativePath':'/'+name},'dataImportance':None,'access':access or [],'advanced':{}})
    response.raise_for_status();row=response.json()
    wait(lambda:api.get('/folders/'+row['id']).raise_for_status().json().get('status',{}).get('state')=='Ready')
    return row


def export(api,folder_id,protocol,rules):
    rows=api.get(f'/folders/{folder_id}/exports').raise_for_status().json()['items']
    row=next(r for r in rows if r['spec']['protocol'].lower()==protocol)
    response=api.put(f'/folders/{folder_id}/exports/{protocol}',json={'rules':rules},etag=str(row['generation']))
    response.raise_for_status()
    wait(lambda:next(r for r in api.get(f'/folders/{folder_id}/exports').raise_for_status().json()['items'] if r['id']==row['id']).get('status',{}).get('state')=='Ready')
    return row


def test_create_grow_adopt_failure_recreate_and_correct_bind(vm_factory,e2e_env):
    vm,a=fresh(vm_factory,e2e_env);api=a.api
    try:
        disks=api.get('/disks').raise_for_status().json()['items']
        selected=sorted((d for d in disks if d['isBlank'] and not d['isSystemDisk']),key=lambda d:d['serial'])
        assert len(selected)==5
        serial=[d['serial'] for d in selected]
        assert api.post('/pools',json={'name':'media','diskSerials':[serial[0]],'typedConfirmation':'ERASE'}).status_code==422
        operation(api,'/pools',{'name':'media','diskSerials':[serial[0]],'typedConfirmation':'ERASE '+serial[0]})
        media=pool(api,'media');wait(lambda:pool(api,'media').get('status',{}).get('state')=='Ready')
        f=folder(api,media['id'],'acceptance-data');path=f['identity']['fileDockPath']
        reference=f"/mnt/filedock/pools/{media['id']}/acceptance-data"
        vm.sudo(f"printf 'pool canary' > {shlex.quote(path)}/canary")
        assert vm.sudo(f'cat {shlex.quote(reference)}/canary').stdout=='pool canary'
        assert vm.sudo(f'stat -c %d:%i {shlex.quote(path)}').stdout==vm.sudo(f'stat -c %d:%i {shlex.quote(reference)}').stdout
        before=vm.sudo(f'df -B1 --output=size {shlex.quote(path)} | tail -n1').stdout.strip()
        operation(api,f"/pools/{media['id']}/disks",{'diskSerial':serial[1],'typedConfirmation':'ERASE '+serial[1]})
        after=vm.sudo(f'df -B1 --output=size {shlex.quote(path)} | tail -n1').stdout.strip();assert int(after)>int(before)
        assert vm.sudo(f'cat {shlex.quote(path)}/canary').stdout=='pool canary'
        # Adopt a known blank fixture disk after formatting it outside FileDock.
        device=selected[4]['devicePath'];vm.sudo('mkfs.ext4 -q '+shlex.quote(device))
        vm.sudo('mkdir -p /var/tmp/fd-adopt; mount '+shlex.quote(device)+' /var/tmp/fd-adopt; printf kept > /var/tmp/fd-adopt/kept; umount /var/tmp/fd-adopt')
        operation(api,'/pools/adopt',{'name':'adopted','diskSerial':serial[4]})
        adopted=pool(api,'adopted');assert vm.sudo(f"cat /mnt/filedock/pools/{adopted['id']}/kept").stdout=='kept'
        # Detach exactly this freshly provisioned VM's first data disk.
        target=vm.sudo('lsblk -ndo KNAME '+shlex.quote(selected[0]['devicePath'])).stdout.strip()
        run('virsh','--connect',vm.env.libvirt_uri,'detach-disk',vm.name,target,'--live')
        wait(lambda:pool(api,'media').get('status',{}).get('state')=='Failed',timeout=60)
        wait(lambda:api.get('/folders/'+f['id']).raise_for_status().json().get('status',{}).get('state')=='Unavailable')
        alerts=api.get('/alerts').raise_for_status().json();assert 'pool.failed' in json.dumps(alerts) and 'acceptance-data' in json.dumps(alerts)
        operation(api,f"/pools/{media['id']}/recreate",{'name':'media','diskSerials':serial[2:4],'typedConfirmation':'ERASE '+' '.join(serial[2:4])})
        assert pool(api,'media')['id']==media['id']
        wait(lambda:api.get('/folders/'+f['id']).raise_for_status().json().get('status',{}).get('state')=='Ready')
        assert vm.sudo(f'test ! -e {shlex.quote(path)}/canary',check=False).returncode==0
    finally:api.close()


def test_protocol_rules_container_bind_and_control_plane_independence(vm_factory,e2e_env):
    vm,a=fresh(vm_factory,e2e_env,disks=1);api=a.api
    try:
        disk=next(d for d in api.get('/disks').raise_for_status().json()['items'] if d['isBlank'] and not d['isSystemDisk'])
        operation(api,'/pools',{'name':'shared','diskSerials':[disk['serial']],'typedConfirmation':'ERASE '+disk['serial']})
        f=folder(api,pool(api,'shared')['id'],'shared-data',['Smb','Nfs','WebDav','Docker']);path=f['identity']['fileDockPath']
        api.post('/share-users',json={'username':'acceptance','password':'acceptance-only','enabled':True}).raise_for_status()
        export(api,f['id'],'smb',[{'principal':'user:acceptance','access':'read-write'}])
        export(api,f['id'],'nfs',[{'client':'127.0.0.1','access':'rw'}])
        api.post('/share-users',json={'username':'reader','password':'reader-only','enabled':True}).raise_for_status()
        export(api,f['id'],'webdav',[{'principal':'user:acceptance','access':'read-write'},{'principal':'user:reader','access':'read'}])
        rows=api.get(f"/folders/{f['id']}/exports").raise_for_status().json()['items'];smb=next(r for r in rows if r['spec']['protocol']=='Smb');dav=next(r for r in rows if r['spec']['protocol']=='WebDav')
        wait(lambda:bool(next(r for r in api.get(f"/folders/{f['id']}/exports").raise_for_status().json()['items'] if r['id']==dav['id']).get('status',{}).get('endpoints')))
        dav=next(r for r in api.get(f"/folders/{f['id']}/exports").raise_for_status().json()['items'] if r['id']==dav['id']);port=next(e['port'] for e in dav['status']['endpoints'] if e['access']=='read-write')
        readport=next(e['port'] for e in dav['status']['endpoints'] if e['access']=='read')
        vm.sudo(f"printf webdav-canary >/var/tmp/fd-upload; curl --fail --user acceptance:acceptance-only -T /var/tmp/fd-upload http://127.0.0.1:{port}/webdav-canary")
        assert vm.sudo(f'cat {shlex.quote(path)}/webdav-canary').stdout=='webdav-canary'
        assert vm.sudo(f'curl -s -o /dev/null -w %{{http_code}} http://127.0.0.1:{port}/webdav-canary').stdout=='401'
        assert vm.sudo(f'curl --fail --user reader:reader-only http://127.0.0.1:{readport}/webdav-canary').stdout=='webdav-canary'
        assert vm.sudo(f'curl --fail --user reader:reader-only -T /var/tmp/fd-upload http://127.0.0.1:{readport}/reader-must-fail',check=False).returncode!=0
        assert vm.sudo(f'curl -s -o /dev/null -w %{{http_code}} --user reader:reader-only http://127.0.0.1:{port}/webdav-canary').stdout=='401'
        # Build a tiny immutable fixture entirely from installed Debian binaries; no registry pull.
        script="""from pathlib import Path
import subprocess,shutil,re
root=Path('/var/tmp/filedock-compose-fixture');root.mkdir(mode=0o700,exist_ok=True)
for name in ['/usr/bin/sleep','/usr/bin/cat']:
 paths=[name]+re.findall(r'/[^\\s()]+',subprocess.check_output(['ldd',name],text=True))
 for path in paths:
  source=Path(path)
  if source.is_file():
   target=root/path.lstrip('/');target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target)
(root/'Dockerfile').write_text('FROM scratch\\nCOPY . /\\nCMD ["/usr/bin/sleep", "infinity"]\\n')
subprocess.run(['docker','build','-q','-t','filedock-acceptance-local',str(root)],check=True)
"""
        vm.sudo("python3 - <<'FD_PY'\n"+script+'FD_PY')
        image=vm.sudo("docker image inspect --format '{{.Id}}' filedock-acceptance-local").stdout.strip()
        # NFS uses the real kernel client and the share user's UID; root remains squashed.
        vm.sudo('mkdir -p /var/tmp/filedock-nfs-client; mount -t nfs -o vers=4 127.0.0.1:'+shlex.quote(path)+' /var/tmp/filedock-nfs-client')
        try:
            vm.sudo('runuser -u acceptance -- touch /var/tmp/filedock-nfs-client/nfs-canary')
            assert vm.sudo('touch /var/tmp/filedock-nfs-client/root-must-fail',check=False).returncode!=0
        finally:vm.sudo('umount /var/tmp/filedock-nfs-client')
        export(api,f['id'],'nfs',[{'client':'127.0.0.1','access':'ro'}])
        vm.sudo('mount -t nfs -o vers=4 127.0.0.1:'+shlex.quote(path)+' /var/tmp/filedock-nfs-client')
        try:assert vm.sudo('runuser -u acceptance -- touch /var/tmp/filedock-nfs-client/readonly-must-fail',check=False).returncode!=0
        finally:vm.sudo('umount /var/tmp/filedock-nfs-client')
        # High-level SMB client comes from the test dependency, never a protocol implementation here.
        import smbclient
        unc=chr(92)*2+vm.ip+chr(92)+smb['id']+chr(92)
        smbclient.register_session(vm.ip,username='acceptance',password='acceptance-only')
        try:
            with smbclient.open_file(unc+'smb-canary',mode='wb') as stream:stream.write(b'smb-canary')
        finally:smbclient.reset_connection_cache()
        assert vm.sudo(f'cat {shlex.quote(path)}/smb-canary').stdout=='smb-canary'
        export(api,f['id'],'smb',[{'principal':'user:acceptance','access':'read'}])
        smbclient.register_session(vm.ip,username='acceptance',password='acceptance-only')
        try:
            with smbclient.open_file(unc+'smb-canary',mode='rb') as stream:assert stream.read()==b'smb-canary'
            with pytest.raises(OSError):
                with smbclient.open_file(unc+'reader-must-fail',mode='wb') as stream:stream.write(b'no')
        finally:smbclient.reset_connection_cache()
        compose=f'services:\n  app:\n    image: {image}\n    command: ["/usr/bin/sleep", "infinity"]\n    volumes:\n      - {path}:/data\n'
        app=api.post('/apps',json={'name':'acceptance','compose':compose,'environment':{},'secrets':[]}).raise_for_status().json()
        wait(lambda:next(r for r in api.get('/apps').raise_for_status().json()['items'] if r['id']==app['id']).get('status',{}).get('state')=='Running')
        vm.sudo("docker compose --project-directory /var/lib/filedock/apps/"+app['id']+" -p fd-acceptance exec -T app cat /data/webdav-canary")
        # Docker access cannot be removed while the Compose app refers to this folder.
        updated=api.get('/folders/'+f['id']).raise_for_status().json()
        assert api.patch('/folders/'+f['id'],json={'identity':updated['identity'],'storageMode':'LocalOnly','reference':updated['reference'],'dataImportance':None,'access':['Smb','Nfs','WebDav'],'advanced':{}},etag=str(updated['generation'])).status_code==422
        outside=api.post('/apps',json={'name':'outside-warning','compose':f'services:\n  app:\n    image: {image}\n    volumes:\n      - external-data:/data\nvolumes:\n  external-data: {{}}\n','environment':{},'secrets':[]}).raise_for_status().json()
        wait(lambda:bool(next(r for r in api.get('/apps').raise_for_status().json()['items'] if r['id']==outside['id']).get('status',{}).get('bindingWarnings')))
        vm.sudo('systemctl stop filedock-management')
        try:assert vm.sudo('cat '+shlex.quote(path)+'/webdav-canary').stdout=='webdav-canary'
        finally:vm.sudo('systemctl start filedock-management')
        vm.sudo('systemctl kill --signal=KILL --kill-whom=main filedock-core')
        assert vm.sudo('cat '+shlex.quote(path)+'/webdav-canary').stdout=='webdav-canary'
    finally:api.close()


def test_small_disk_system_pool_is_unavailable(vm_factory,e2e_env):
    vm,a=fresh(vm_factory,e2e_env,small=True)
    try:
        status=a.api.get('/pools/system').raise_for_status().json();assert not status.get('available',False)
        assert '100 GB reserve' in status.get('unavailableReason','')
    finally:a.api.close()


@pytest.mark.failure_injection
def test_system_pool_reserve_hard_cap_and_no_automatic_user_data_deletion(vm_factory,e2e_env):
    vm,a=fresh(vm_factory,e2e_env,disks=1);api=a.api
    try:
        wait(lambda:api.get('/pools/system').raise_for_status().json().get('available',False))
        system=api.get('/pools/system').raise_for_status().json();f=folder(api,system['poolId'],'reserve-data');path=f['identity']['fileDockPath']
        vm.sudo(f"printf KEEP > {shlex.quote(path)}/user-data")
        before=vm.sudo(f'sha256sum {shlex.quote(path)}/user-data').stdout
        result=api.put('/pools/system/limits',json={'maxUsagePercent':10,'reservedFreeGb':100,'acknowledgedInterruption':True},etag=str(system['generation'])).raise_for_status().json()
        assert api.wait_operation(result['id'])['state']=='Succeeded'
        wait(lambda:api.get('/folders/'+f['id']).raise_for_status().json().get('status',{}).get('state')=='Ready')
        fill="import os\np="+repr(path)+"\ns=os.statvfs(p);size=max(0,(s.f_bfree-8)*s.f_frsize)\nf=os.open(p+'/hard-cap-fill',os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)\nos.posix_fallocate(f,0,size);os.close(f)\n"
        vm.sudo("python3 - <<'FD_PY'\n"+fill+'FD_PY')
        try:
            assert vm.sudo('dd if=/dev/zero of='+shlex.quote(path)+'/overflow bs=1M count=64 conv=fsync',check=False).returncode!=0
            assert before==vm.sudo(f'sha256sum {shlex.quote(path)}/user-data').stdout
            latest=api.get('/pools/system').raise_for_status().json()
            reserve=int(vm.sudo("python3 -c "+shlex.quote("import os;s=os.statvfs('/var/lib/filedock');print(s.f_blocks*s.f_frsize//1000000000-2)")).stdout.strip())
            refused=api.put('/pools/system/limits',json={'maxUsagePercent':10,'reservedFreeGb':reserve,'acknowledgedInterruption':True},etag=str(latest['generation']))
            assert refused.status_code==422 and 'system_pool.below_usage' in refused.text,refused.text
        finally:vm.sudo('rm -f '+shlex.quote(path)+'/hard-cap-fill '+shlex.quote(path)+'/overflow')
        # Consume only a named filler on this disposable VM, stopping above the configured reserve.
        script="import os,json\ns=os.statvfs('/var/lib/filedock');free=s.f_bavail*s.f_frsize\nreserve=100_000_000_000\nguard=min(5_000_000_000,s.f_blocks*s.f_frsize//50)\nsize=max(0,free-reserve-guard//2)\nf=os.open('/var/tmp/filedock-reserve-fill',os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)\nos.posix_fallocate(f,0,size);os.close(f)\n"
        vm.sudo("python3 - <<'FD_PY'\n"+script+'FD_PY')
        try:
            wait(lambda:api.get('/pools/system').raise_for_status().json().get('writable')=='FULL_RESERVE',timeout=15)
            assert vm.sudo(f'touch {shlex.quote(path)}/should-fail',check=False).returncode!=0
            assert before==vm.sudo(f'sha256sum {shlex.quote(path)}/user-data').stdout
            assert vm.sudo("stat -c %s /var/lib/filedock/system-pool/pool.img").returncode==0
        finally:vm.sudo('rm -f /var/tmp/filedock-reserve-fill')
        wait(lambda:api.get('/pools/system').raise_for_status().json().get('writable')=='WRITABLE',timeout=15)
        # Out-of-range limits are refused; user data remains unchanged.
        latest=api.get('/pools/system').raise_for_status().json()
        response=api.put('/pools/system/limits',json={'maxUsagePercent':9,'reservedFreeGb':100,'acknowledgedInterruption':True},etag=str(latest['generation']));assert response.status_code==422
        assert before==vm.sudo(f'sha256sum {shlex.quote(path)}/user-data').stdout
    finally:api.close()


@pytest.mark.failure_injection
def test_data_and_system_pressure_alerts_and_atomic_refusal(vm_factory,e2e_env):
    vm,a=fresh(vm_factory,e2e_env,disks=1);api=a.api
    try:
        disk=next(d for d in api.get('/disks').raise_for_status().json()['items'] if d['isBlank'] and not d['isSystemDisk'])
        operation(api,'/pools',{'name':'pressure','diskSerials':[disk['serial']],'typedConfirmation':'ERASE '+disk['serial']})
        row=pool(api,'pressure');f=folder(api,row['id'],'pressure-data');path=f['identity']['fileDockPath']
        vm.sudo('printf KEEP > '+shlex.quote(path)+'/user-data');before=vm.sudo('sha256sum '+shlex.quote(path)+'/user-data').stdout
        def allocate(target,percent):
            script="import os\np="+repr(target)+"\ns=os.statvfs(p);total=s.f_blocks*s.f_frsize;used=(s.f_blocks-s.f_bfree)*s.f_frsize\nsize=max(0,int(total*"+repr(percent)+")-used)\nf=os.open(p+'/filedock-pressure-fill',os.O_CREAT|os.O_WRONLY,0o600)\nos.posix_fallocate(f,0,os.fstat(f).st_size+size);os.close(f)\n"
            vm.sudo("python3 - <<'FD_PY'\n"+script+'FD_PY')
        allocate(path,.91)
        wait(lambda:any(x.get('kind')=='storage.disk_pressure' and x.get('severity')=='Warning' for x in api.get('/alerts').raise_for_status().json()['items']),timeout=65)
        # Extend the same named filler to critical pressure.
        allocate(path,.97)
        wait(lambda:any(x.get('kind')=='storage.disk_pressure' and x.get('severity')=='Critical' for x in api.get('/alerts').raise_for_status().json()['items']),timeout=65)
        assert before==vm.sudo('sha256sum '+shlex.quote(path)+'/user-data').stdout
        vm.sudo('rm -f '+shlex.quote(path)+'/filedock-pressure-fill')
        # Fill only a named OS-disk test file, leaving enough room to read/authenticate the response.
        script="import os\ns=os.statvfs('/var/tmp');size=max(0,s.f_bavail*s.f_frsize-8*1024*1024)\nf=os.open('/var/tmp/filedock-system-pressure-fill',os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600);os.posix_fallocate(f,0,size);os.close(f)\n"
        count=vm.sudo("sqlite3 /var/lib/filedock-core/core.db 'SELECT change_seq FROM state_meta WHERE id=1'").stdout.strip() if vm.sudo('command -v sqlite3',check=False).returncode==0 else None
        vm.sudo("python3 - <<'FD_PY'\n"+script+'FD_PY')
        try:
            response=api.post('/apps',json={'name':'must-not-commit','compose':'services:\n  app:\n    image: example.invalid/image:1\n','environment':{},'secrets':[]})
            assert response.status_code==507,response.text
            assert 'storage.disk_full' in response.text
            wait(lambda:any(x.get('kind')=='storage.disk_pressure' and x.get('severity')=='Critical' for x in api.get('/alerts').raise_for_status().json()['items']),timeout=65)
        finally:vm.sudo('rm -f /var/tmp/filedock-system-pressure-fill')
        if count is not None:assert count==vm.sudo("sqlite3 /var/lib/filedock-core/core.db 'SELECT change_seq FROM state_meta WHERE id=1'").stdout.strip()
        assert not any(x['spec']['name']=='must-not-commit' for x in api.get('/apps').raise_for_status().json()['items'])
        assert before==vm.sudo('sha256sum '+shlex.quote(path)+'/user-data').stdout
    finally:
        vm.sudo('rm -f /var/tmp/filedock-system-pressure-fill',check=False)
        api.close()
