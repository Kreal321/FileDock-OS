"""US5 configuration backup recovery on another fresh VM, without taking any remote writer role."""
import json
import pytest
from state_helpers import setup,snapshot,wait

@pytest.mark.two_vms
def test_backup_file_restores_administrators_on_fresh_hardware(installed_vm,vm_factory):
    original=setup(installed_vm);snapshot(original)
    try:
        backup=original.api.get('/state-protection/backup-file').raise_for_status().content
        target=vm_factory();target.install('--yes');api=target.api()
        try:
            code=json.loads(target.sudo('filedockctl --json setup-code').stdout)['code']
            response=api.http.post('/setup/restore/file',data={'setupCode':code,'recoveryKey':original.recovery_key},files={'file':('filedock.fdbackup',backup,'application/octet-stream')})
            response.raise_for_status();assert response.json()['roleRecoveryReader']
            snapshots=api.get('/setup/restore/snapshots').raise_for_status().json()['snapshots'];selected=next(s for s in snapshots if s.get('newestValid'))
            api.post('/setup/restore',json={'snapshotId':selected['snapshotId']}).raise_for_status()
            wait(lambda:api.post('/auth/sign-in',json={'username':original.username,'password':original.password}).status_code==200,timeout=120)
            report=api.get('/restore/report').raise_for_status().json();assert report['snapshotTakenAt']
            locations=api.get('/locations').raise_for_status().json().get('locations',[])
            assert all(l['role'] in ('RECOVERY_READER','RecoveryReader') for l in locations)
        finally:api.close()
    finally:original.api.close()
