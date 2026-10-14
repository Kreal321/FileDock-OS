"""US6 acceptance on disposable Debian VMs. The data fixture never provisions a real disk."""
import json
import os
import shlex
import time
import uuid
from pathlib import Path
import pytest
from state_helpers import setup, snapshot, wait

PACKAGES=Path(os.environ.get('FD_E2E_EXTENSION_PACKAGES',str(Path(__file__).resolve().parents[2]/'dist/fixtures')))

def inspect(api,name):
    path=PACKAGES/(name+'.fdext')
    if not path.is_file(): pytest.skip('Build sample packages with tests/fixtures/build.sh on Debian first.')
    with path.open('rb') as file:
        response=api.http.post('/extensions/inspect',files={'file':(path.name,file,'application/octet-stream')},headers=api._csrf())
    response.raise_for_status();value=response.json();assert value['valid'],value
    return value

def install(api,inspection,bindings=None,ack=False):
    response=api.post('/extensions',json={'stagedPackageId':inspection['stagedPackageId'],'approvedPermissions':inspection['requestedPermissions'],'acknowledgedUnsigned':True,'dataBindings':bindings or {},'acknowledgedUnprotectedData':ack})
    response.raise_for_status();operation=api.wait_operation(response.json()['id']);assert operation['state']=='Succeeded',operation

def subsystem(api,id):
    return next(s for s in api.get('/subsystems').raise_for_status().json()['subsystems'] if s['subsystemId']==id)

def data_fixture(vm):
    if vm.env.existing_host:pytest.skip('Synthetic pool/folder fixture is restricted to disposable libvirt VMs.')
    folder=str(uuid.uuid4());pool=str(uuid.uuid4());path='/storage/fd-extension-test-'+folder[:8]
    vm.sudo(f'mkdir -p {path}; mount -t tmpfs -o size=8m,mode=0750 tmpfs {path}')
    document=[('Pool',pool,{'name':'extension-test-pool','kind':'Adopted','layers':[],'memberDisks':[{'serial':'fixture-only-not-a-device'}]}),('ManagedFolder',folder,{'name':'extension-test-data','filedockPath':path,'storageMode':'LocalOnly','reference':{'poolId':pool,'relativePath':'data'},'access':[],'advanced':{}})]
    # Observed provider fixture, solely for sandbox tests; no production pool creation is certified.
    script="import sqlite3,json,datetime\nc=sqlite3.connect('/var/lib/filedock-core/core.db')\nnow=datetime.datetime.now(datetime.timezone.utc).isoformat()\nfor kind,id,spec in json.loads("+repr(json.dumps(document))+"):\n c.execute(\"INSERT INTO resource(kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at) VALUES(?,?,1,?,NULL,0,1,?,?)\",(kind,id,json.dumps(spec),now,now))\nc.commit()\n"
    vm.sudo("python3 - <<'FD_PY'\n"+script+'FD_PY')
    return folder,pool,path

def test_three_runtimes_bindings_and_package_copies(installed_vm):
    appliance=setup(installed_vm);api=appliance.api
    try:
        for name in ['sample-native','sample-systemd']:install(api,inspect(api,name))
        # A local, immutable image archive is built alongside the package.
        image=PACKAGES/'sample-docker-image.tar'
        if not image.exists():pytest.skip('Build the Docker image fixture alongside the packages.')
        installed_vm.scp([str(image)],'/tmp/filedock-extension-test-image.tar')
        installed_vm.sudo('docker load -i /tmp/filedock-extension-test-image.tar; rm /tmp/filedock-extension-test-image.tar')
        folder,pool,path=data_fixture(installed_vm)
        package=inspect(api,'sample-docker')
        body={'stagedPackageId':package['stagedPackageId'],'approvedPermissions':package['requestedPermissions'],'acknowledgedUnsigned':True,'dataBindings':{},'acknowledgedUnprotectedData':False}
        assert api.post('/extensions',json=body).status_code==422
        body['dataBindings']={'data':folder};response=api.post('/extensions',json=body);assert response.status_code==422 and 'extension.data_unprotected' in response.text
        install(api,package,{'data':folder},True)
        assert installed_vm.sudo(f'cat {path}/sample.txt').stdout.strip()=='sample capability used'
        assert 'SANDBOX_DENIED' in installed_vm.sudo('docker logs fd-extension-sample-docker 2>&1').stdout
        assert not subsystem(api,'sample-docker')['dataProtected']
        # Deletion goes through real validation, rather than deleting the fixture behind the Core.
        native_hash=inspect(api,'sample-native')['stagedPackageId']
        helper=f'/var/lib/filedock/extensions/runtimes/{native_hash}/sample'
        assert 'PASS' in installed_vm.sudo(f'{helper} --assert-bound-delete-refused {folder}').stdout
        snapshot(appliance)
        result=installed_vm.sudo("python3 - <<'FD_PY'\nfrom pathlib import Path\nimport json\nfor p in Path('/var/backups/filedock/appliance-state').glob('appliance/*/index.json'):\n index=json.loads(p.read_text()); hashes=set(h for s in index['snapshots'] for h in s.get('packages',[]));assert len(hashes)>=3\n for h in hashes: assert (p.parent/'packages'/f'{h}.fdpkg').is_file()\nFD_PY")
        assert result.returncode==0
        assert api.delete('/extensions/docker').status_code==422
    finally:api.close()

def test_twenty_kills_do_not_restart_core_and_broken_upgrade_rolls_back(installed_vm):
    appliance=setup(installed_vm);api=appliance.api
    try:
        install(api,inspect(api,'sample-native'))
        before=installed_vm.sudo('systemctl show filedock-core --property=MainPID --property=NRestarts').stdout
        for _ in range(20):
            output=installed_vm.sudo('systemctl show filedock-extension-sample-native --property=MainPID --value').stdout.strip();assert output and output!='0'
            installed_vm.sudo('kill -KILL '+shlex.quote(output))
            deadline=time.monotonic()+120
            while time.monotonic()<deadline:
                state=subsystem(api,'sample-native')['state']
                if state=='Failed':
                    op=api.post('/extensions/sample-native/restart',json={}).raise_for_status().json();assert api.wait_operation(op['id'])['state']=='Succeeded'
                current=installed_vm.sudo('systemctl show filedock-extension-sample-native --property=MainPID --value',check=False).stdout.strip()
                if current not in ('','0',output) and subsystem(api,'sample-native')['state']=='Running':break
                time.sleep(.5)
            else:pytest.fail('Extension did not recover after a kill')
        assert before==installed_vm.sudo('systemctl show filedock-core --property=MainPID --property=NRestarts').stdout
        broken=inspect(api,'sample-native-broken');op=api.post('/extensions/sample-native/upgrade',json={'stagedPackageId':broken['stagedPackageId'],'approvedPermissions':broken['requestedPermissions'],'acknowledgedUnsigned':True}).raise_for_status().json()
        result=api.wait_operation(op['id']);assert result['state']=='RolledBack',result
        wait(lambda:subsystem(api,'sample-native')['version']=='1.0.0' and subsystem(api,'sample-native')['state']=='Running')
    finally:api.close()
