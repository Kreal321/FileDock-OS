"""Phase 6–7 helpers. Use only disposable test VMs; no user credentials are embedded."""
from dataclasses import dataclass, field
import json
import time
import uuid
import pytest

@dataclass
class StateAppliance:
    vm: object
    api: object
    recovery_key: str = field(repr=False)
    password: str = field(repr=False)
    username: str = 'state-admin'

def wait(check, timeout=65):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        if check(): return
        time.sleep(.5)
    pytest.fail('Appliance condition did not become true within the deadline')

def setup(vm):
    api=vm.api()
    assert api.get('/system/status').status_code==423,'Use a fresh disposable VM for Phase 6–7 acceptance.'
    code=json.loads(vm.sudo('filedockctl --json setup-code').stdout)['code']
    password='test-'+uuid.uuid4().hex
    response=api.post('/setup/complete',json={'code':code,'applianceName':'state-e2e','adminUsername':'state-admin','adminPassword':password,'adminPasswordConfirmation':password})
    response.raise_for_status();key=response.json()
    api.post('/setup/recovery-key/confirm',json={'keyId':key['keyId']}).raise_for_status()
    wait(lambda:api.get('/system/core-health').raise_for_status().json()['canMutate'])
    wait(lambda:any(c.get('lastSnapshotAt') for c in api.get('/state-protection').raise_for_status().json().get('copies',[])))
    return StateAppliance(vm,api,key['recoveryKey'],password)

def snapshot(appliance):
    op=appliance.api.post('/state-protection/snapshot',json={}).raise_for_status().json()
    result=appliance.api.wait_operation(op['id']);assert result['state']=='Succeeded'
    return appliance.api.get('/state-protection').raise_for_status().json()

def reset_configuration(vm):
    if vm.env.existing_host: pytest.skip('Reinstall/reset scenarios require a disposable libvirt VM, never an existing PVE VM.')
    vm.sudo('systemctl stop filedock-core.socket filedock-core.service filedock-management.service; rm -rf /var/lib/filedock-core /var/lib/filedock-management /var/lib/filedock; systemctl start filedock-core.socket filedock-core.service filedock-management.service')
    api=vm.api();wait(lambda:api.get('/health').status_code==200);return api
