"""SC-010 baseline on disposable appliances. Never power-cut an existing PVE VM.

Restart checks exercise installed daemons and validate both SQLite stores, JSON,
secret-dependent snapshot generation, and committed canary resources. Data-plane
continuity with active shares/apps is covered separately by test_independence.py.
"""
from concurrent.futures import ThreadPoolExecutor
import json
import threading
import time
import uuid

import httpx
import pytest
from state_helpers import setup, snapshot
from test_independence import wait

UNITS=('filedock-management.service','filedock-core.service','docker.service','smbd.service',
       'nfs-server.service','filedock-s3.service','filedock-rclone.service')


def integrity(vm):
    vm.sudo("""python3 - <<'PY2'
import sqlite3,json
for path in ['/var/lib/filedock-core/core.db','/var/lib/filedock-management/app.db']:
 with sqlite3.connect(path) as c:
  assert c.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
  assert not c.execute('PRAGMA foreign_key_check').fetchall()
with sqlite3.connect('/var/lib/filedock-core/core.db') as c:
 for spec,status in c.execute('SELECT spec,status FROM resource'):
  json.loads(spec)
  if status:json.loads(status)
PY2""")


def healthy(api):
    return api.get('/system/core-health').raise_for_status().json()['canMutate'] and all(
        c['state']=='Healthy' for c in api.get('/system/status').raise_for_status().json()['components'])


def connection(api,name):
    return api.post('/connections',json={'name':name,'providerFamily':'s3-compatible','definition':{
        'type':'s3','endpoint':'https://example.invalid','region':'us-east-1','bucket':'fixture',
        'addressingStyle':'Path','accessKey':'synthetic-access','secretKey':'synthetic-secret'}})


@pytest.mark.failure_injection
def test_each_runtime_restart_preserves_committed_configuration(installed_vm):
    a=setup(installed_vm);api=a.api
    try:
        canary=connection(api,'failure-'+uuid.uuid4().hex).raise_for_status().json()
        for unit in UNITS:
            installed_vm.sudo('systemctl restart '+unit)
            wait(lambda:healthy(api),timeout=180)
            integrity(installed_vm)
            row=api.get('/connections/'+canary['id']).raise_for_status().json()
            assert row['generation']==canary['generation'] and row['spec']==canary['spec']
            snapshot(a)
        # Kopia is a supervised adapter, not an independently enabled systemd unit.
        installed_vm.sudo('systemctl restart filedock-core.service')
        wait(lambda:healthy(api),timeout=180)
        assert any(c['id']=='kopia' and c['state']=='Healthy' for c in api.get('/system/status').json()['components'])
    finally:api.close()


@pytest.mark.failure_injection
def test_power_cut_during_configuration_writes_is_atomic(installed_vm):
    if installed_vm.env.existing_host:
        pytest.skip('Power-cut testing requires an owned disposable libvirt domain.')
    a=setup(installed_vm);api=a.api;acknowledged=[];prefix='power-'+uuid.uuid4().hex
    try:
        for cycle in range(3):
            started=threading.Event()
            def writes():
                started.set()
                for n in range(1000):
                    try:
                        response=connection(api,f'{prefix}-{cycle}-{n}')
                        if response.is_success:acknowledged.append(response.json())
                        else:break
                    except httpx.HTTPError:break
            with ThreadPoolExecutor(max_workers=1) as pool:
                pending=pool.submit(writes);assert started.wait(5);time.sleep(.15)
                installed_vm.virsh('destroy');installed_vm.virsh('start')
                pending.result(timeout=60)
            installed_vm.wait_for_ssh(timeout=180);wait(lambda:healthy(api),timeout=180)
            integrity(installed_vm)
            for expected in acknowledged:
                row=api.get('/connections/'+expected['id']).raise_for_status().json()
                assert row['spec']==expected['spec'] and row['generation']==expected['generation']
            snapshot(a)
        assert acknowledged,'No acknowledged writes were exercised; inconclusive power-cut run'
    finally:api.close()
