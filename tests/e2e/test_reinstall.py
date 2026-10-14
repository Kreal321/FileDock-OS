"""US5 recovery-key-only reinstall on disposable VM configuration state.
Storage provisioning and the package provider are separately exercised after US9/US6.
"""
import time
from state_helpers import setup,snapshot,reset_configuration,wait

def test_reinstall_requires_only_the_key_and_retains_the_report(installed_vm):
    original=setup(installed_vm);snapshot(original);original.api.close()
    api=reset_configuration(installed_vm);started=time.monotonic()
    try:
        found=api.get('/setup/restore/local').raise_for_status().json()['appliances'];assert found
        token=found[0]['localSourceToken']
        assert api.post('/setup/restore/local',json={'localSourceToken':token,'recoveryKey':'00000-00000-00000-00000-00000'}).status_code==409
        source=api.post('/setup/restore/local',json={'localSourceToken':token,'recoveryKey':original.recovery_key}).raise_for_status().json()
        assert source['roleRecoveryReader']
        choices=api.get('/setup/restore/snapshots').raise_for_status().json()['snapshots'];selected=next(s for s in choices if s.get('newestValid'))
        api.post('/setup/restore',json={'snapshotId':selected['snapshotId']}).raise_for_status()
        wait(lambda:api.post('/auth/sign-in',json={'username':original.username,'password':original.password}).status_code==200,timeout=120)
        assert time.monotonic()-started<1800
        report=api.get('/restore/report').raise_for_status().json();assert report['snapshotTakenAt']
        api.post('/restore/report/dismiss',json={}).raise_for_status();assert api.get('/restore/report').status_code==204
    finally:api.close()
