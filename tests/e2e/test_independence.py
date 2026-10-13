"""US3 acceptance using a disposable subsystem until US9 supplies shares and apps.

Never run power-cut tests on an existing PVE VM: libvirt owns the disposable domains.
"""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import time

import httpx
import pytest
from conftest import REPO

FIXTURE=REPO/'tests/fixtures/phase5'

def wait(check,timeout=30):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        try:
            if check():return
        except (httpx.HTTPError,OSError):pass
        time.sleep(.1)
    pytest.fail('appliance condition timed out')

@pytest.fixture
def appliance(installed_vm):
    vm=installed_vm
    api=vm.api()
    code=json.loads(vm.sudo('filedockctl setup-code --json').stdout)['code']
    response=api.post('/setup/complete',json={'code':code,'applianceName':'phase5-e2e','adminUsername':'phase5-admin','adminPassword':'test-only','adminPasswordConfirmation':'test-only'}).raise_for_status()
    api.post('/setup/recovery-key/confirm',json={'keyId':response.json()['keyId']}).raise_for_status()
    vm.ssh('mkdir -p ~/phase5-fixture')
    vm.copy_to(*sorted(FIXTURE.iterdir()),remote='phase5-fixture/')
    vm.sudo('mkdir -p /var/lib/filedock-phase5-test/data; cp /home/'+vm.env.ssh_user+'/phase5-fixture/sample_server.py /var/lib/filedock-phase5-test/; cp /home/'+vm.env.ssh_user+'/phase5-fixture/filedock-phase5-sample.service /etc/systemd/system/; cp /home/'+vm.env.ssh_user+'/phase5-fixture/sample.yaml /usr/share/filedock/subsystems/phase5-sample.yaml; systemctl daemon-reload; systemctl enable --now filedock-phase5-sample; systemctl restart filedock-core')
    wait(lambda:api.get('/system/core-health').raise_for_status().json()['canMutate'])
    wait(lambda:all(c['state']=='Healthy' for c in api.get('/system/status').raise_for_status().json()['components']),timeout=180)
    yield vm,api
    vm.sudo('systemctl stop filedock-core.socket filedock-core.service; systemctl disable --now filedock-phase5-sample; rm -f /usr/share/filedock/subsystems/phase5-sample.yaml /etc/systemd/system/filedock-phase5-sample.service; systemctl daemon-reload',check=False)
    vm.sudo("python3 - <<'PY2'\nimport sqlite3\nwith sqlite3.connect('/var/lib/filedock-core/core.db') as c:\n c.execute(\"DELETE FROM subsystem WHERE id='phase5-sample'\")\n c.execute(\"DELETE FROM setting WHERE key='supervision.phase5-sample'\")\nPY2",check=False)
    vm.sudo('rm -rf /var/lib/filedock-phase5-test; systemctl start filedock-core.socket filedock-core.service',check=False)
    api.close()


def test_management_and_core_are_not_in_the_data_plane(appliance):
    vm,api=appliance
    address=f'http://{vm.ip}:18025/'
    httpx.put(address,content=b'before').raise_for_status()
    pid=vm.sudo('systemctl show filedock-phase5-sample -p MainPID --value').stdout.strip()
    try:
        vm.sudo('systemctl stop filedock-management')
        httpx.put(address,content=b'management-down').raise_for_status()
        assert httpx.get(address).content==b'management-down'
        vm.sudo('systemctl kill -s KILL filedock-core')
        wait(lambda:httpx.get(address).content==b'management-down')
        assert vm.sudo('systemctl show filedock-phase5-sample -p MainPID --value').stdout.strip()==pid,'Core restart must adopt the healthy data plane without restarting it'
    finally:vm.sudo('systemctl start filedock-management')
    wait(lambda:api.get('/system/core-health').raise_for_status().json()['canMutate'])


def test_crash_loop_backoff_failed_diagnostics_and_manual_recovery(appliance):
    vm,api=appliance
    vm.sudo('touch /var/lib/filedock-phase5-test/data/crash')
    def sample():
        return next(c for c in api.get('/system/status').raise_for_status().json()['components'] if c['id']=='phase5-sample')
    wait(lambda:sample()['state']=='Failed',timeout=120)
    state=vm.sudo("python3 - <<'PY2'\nimport sqlite3,json\nc=sqlite3.connect('/var/lib/filedock-core/core.db')\nprint(c.execute(\"SELECT value FROM setting WHERE key='supervision.phase5-sample'\").fetchone()[0])\nPY2").stdout
    supervision=json.loads(state)
    assert len(supervision['failures'])==5
    assert supervision['diagnostics']
    attempts=supervision['failures']
    assert attempts[-1]-attempts[0]>=10,'restart attempts must back off'
    vm.sudo('systemctl restart filedock-core')
    wait(lambda:sample()['state']=='Failed')
    vm.sudo('rm /var/lib/filedock-phase5-test/data/crash; systemctl reset-failed filedock-phase5-sample; systemctl start filedock-phase5-sample')
    wait(lambda:sample()['state']=='Healthy')


def test_browser_receives_live_failure_and_core_recovery(appliance):
    vm,_=appliance
    env=__import__('os').environ.copy()
    env.update(FD_BASE_URL=f'http://{vm.ip}:8021',FD_TEST_ADMIN='phase5-admin',FD_TEST_PASSWORD='test-only',FD_PHASE5_SSH_HOST=vm.ip,FD_PHASE5_SSH_USER=vm.env.ssh_user,FD_PHASE5_SSH_KEY=str(vm.ssh_key))
    result=subprocess.run(['pnpm','-C',str(REPO/'web'),'exec','playwright','test','phase5.spec.ts'],env=env,capture_output=True,text=True,timeout=120)
    assert result.returncode==0,result.stdout+result.stderr


@pytest.mark.failure_injection
def test_reboot_and_power_cut_recover_with_intact_state(appliance):
    vm,api=appliance
    if vm.env.existing_host:pytest.skip('power cuts require a disposable libvirt VM')
    started=time.monotonic()
    vm.sudo('reboot',check=False,timeout=15)
    vm.wait_for_ssh(timeout=180)
    wait(lambda:all(c['state']=='Healthy' for c in api.get('/system/status').raise_for_status().json()['components']),timeout=max(1,180-(time.monotonic()-started)))
    assert time.monotonic()-started<180
    # Cut power while a durable reconciliation operation is being started/executed.
    with ThreadPoolExecutor(max_workers=1) as executor:
        change=executor.submit(api.post,'/operations/reconcile',json={})
        vm.virsh('destroy');vm.virsh('start');vm.wait_for_ssh(timeout=180)
        try:change.result()
        except httpx.HTTPError:pass
    wait(lambda:api.get('/system/core-health').raise_for_status().json()['canMutate'],timeout=180)
    integrity=vm.sudo("python3 - <<'PY2'\nimport sqlite3,json\nfor path in ['/var/lib/filedock-core/core.db','/var/lib/filedock-management/app.db']:\n c=sqlite3.connect(path);assert c.execute('PRAGMA integrity_check').fetchone()[0]=='ok';assert not c.execute('PRAGMA foreign_key_check').fetchall()\nc=sqlite3.connect('/var/lib/filedock-core/core.db')\nfor spec, in c.execute('SELECT spec FROM resource'):json.loads(spec)\nPY2")
    assert integrity.returncode==0
    assert api.get('/system/status').json()['applianceName']=='phase5-e2e'
