"""US7 notification acceptance. Requires freshly installed disposable VM and Linux-built fixtures."""
import json,time
from state_helpers import setup,wait
from test_extensions import data_fixture,inspect,install

def test_critical_alert_is_visible_and_delivered_within_one_minute(installed_vm):
    appliance=setup(installed_vm);api=appliance.api
    try:
        assert api.get('/system/status').raise_for_status().json()['notificationChannelMissing']
        folder,pool,path=data_fixture(installed_vm,api)
        install(api,inspect(api,'notify-test'),{'data':folder},True)
        native=inspect(api,'sample-native');native_hash=native['stagedPackageId'];install(api,native)
        channel=api.post('/notification-channels',json={'name':'Acceptance recorder','subsystemId':'notify-test','minSeverity':'Warning','config':{},'secretRefs':[]}).raise_for_status().json()
        assert not api.get('/system/status').raise_for_status().json()['notificationChannelMissing']
        assert api.post('/notification-channels/'+channel['id']+'/test',json={}).status_code==200
        started=time.monotonic()
        installed_vm.sudo(f'set -a; . /run/filedock/extension-env/sample-native.env; set +a; /var/lib/filedock/extensions/runtimes/{native_hash}/sample --raise-test-alert')
        def delivered():
            text=installed_vm.sudo(f'cat {path}/deliveries.jsonl',check=False).stdout
            return any(row.get('severity')=='Critical' for row in (json.loads(l) for l in text.splitlines()))
        wait(delivered,timeout=59)
        assert time.monotonic()-started<60
        alerts=api.get('/alerts?state=Active&minSeverity=Critical').raise_for_status().json()['items'];alert=next(a for a in alerts if a['kind']=='phase9.test_critical')
        api.post('/alerts/'+alert['id']+'/ack',json={}).raise_for_status()
        wait(lambda: 'acknowledged' in installed_vm.sudo(f'cat {path}/deliveries.jsonl').stdout)
        # UI visibility is separately exercised by the frontend critical-banner test.
    finally:api.close()

def test_audit_survives_journal_vacuum_and_diagnostics_redact_canaries(installed_vm):
    import pytest,uuid
    if installed_vm.env.existing_host:pytest.skip('Journal vacuum is restricted to a fresh disposable libvirt VM.')
    appliance=setup(installed_vm);api=appliance.api
    try:
        token=api.post('/tokens',json={'name':'audit-retention-check','scope':'ReadOnly'}).raise_for_status().json()
        before=api.get('/audit?action=token.created').raise_for_status().json()['items'];assert before
        canary='FD-NOTIFY-CANARY-'+uuid.uuid4().hex
        api.put('/secrets/notification/audit-check/key',json={'value':canary,'consumers':[]}).raise_for_status()
        # Exercise the same journal retention machinery without filling a whole gigabyte.
        installed_vm.sudo('journalctl --rotate; journalctl --vacuum-size=1M')
        after=api.get('/audit?action=token.created').raise_for_status().json()['items'];assert {e['id'] for e in before}<={e['id'] for e in after}
        script="import subprocess\nfrom pathlib import Path\np=Path('/var/tmp/fd-diagnostics-acceptance.tar.zst')\nassert not p.exists()\nsubprocess.run(['filedockctl','diagnostics',str(p)],check=True)\ntry:\n data=subprocess.check_output(['zstd','-q','-d','-c',str(p)])\n assert "+repr(canary.encode())+" not in data\n print('PASS diagnostic canary absent')\nfinally:p.unlink(missing_ok=True)\n"
        result=installed_vm.sudo("python3 - <<'FD_PY'\n"+script+'FD_PY');assert 'PASS' in result.stdout
    finally:api.close()
