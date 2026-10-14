"""US4 configuration copies and scheduler acceptance. Remote storage needs a TLS test service."""
import json
import time
import uuid
from state_helpers import setup,snapshot,wait

def test_local_copy_and_authentication_does_not_change_configuration(installed_vm):
    appliance=setup(installed_vm);api=appliance.api
    try:
        protection=snapshot(appliance)
        assert any(c['kind'] in ('LOCAL_SYSTEM_DISK','LocalSystemDisk') and c.get('lastSnapshotAt') for c in protection['copies'])
        before=installed_vm.sudo("find /var/backups/filedock/appliance-state -name '*.fdstate' -printf '%f\\n' | sort").stdout
        api.post('/auth/sign-out',json={}).raise_for_status()
        api.post('/auth/sign-in',json={'username':appliance.username,'password':appliance.password}).raise_for_status()
        time.sleep(35)
        assert before==installed_vm.sudo("find /var/backups/filedock/appliance-state -name '*.fdstate' -printf '%f\\n' | sort").stdout
        started=time.monotonic();old=[c.get('lastSnapshotAt') for c in protection['copies']]
        password='test-'+uuid.uuid4().hex
        api.post('/admins',json={'username':'another-admin','password':password,'passwordConfirmation':password}).raise_for_status()
        wait(lambda:any(c.get('lastSnapshotAt') not in old for c in api.get('/state-protection').raise_for_status().json()['copies']),timeout=60)
        assert time.monotonic()-started<60
        # Runtime histories must not be included in the published Management section.
        document=installed_vm.sudo("python3 - <<'PY2'\nimport sqlite3,json\nc=sqlite3.connect('/var/lib/filedock-core/core.db');v=json.loads(c.execute(\"SELECT value FROM setting WHERE key='management.config'\").fetchone()[0]);print(json.dumps({'keys':sorted(v),'adminCount':len(v['administrators'])}))\nPY2").stdout
        assert json.loads(document)['adminCount']==2
        assert not {'sessions','audit','setupState'}.intersection(json.loads(document)['keys'])
    finally:api.close()

def test_conditional_object_storage_claim_and_machine_loss_coverage(installed_vm):
    import os
    import boto3
    import pytest
    endpoint=os.environ.get('FD_E2E_STATE_ENDPOINT','')
    if not endpoint.startswith('https://'):
        pytest.skip('Set FD_E2E_STATE_ENDPOINT to a trusted TLS MinIO/S3 service to run remote-copy acceptance.')
    access=os.environ['FD_E2E_STATE_ACCESS_KEY'];secret=os.environ['FD_E2E_STATE_SECRET_KEY']
    client=boto3.client('s3',endpoint_url=endpoint,aws_access_key_id=access,aws_secret_access_key=secret,region_name='us-east-1')
    bucket='fd-state-'+uuid.uuid4().hex[:12];client.create_bucket(Bucket=bucket)
    appliance=setup(installed_vm)
    try:
        created=appliance.api.post('/state-protection/connections',json={'name':'state-test','endpoint':endpoint,'region':'us-east-1','bucket':bucket,'accessKey':access,'secretKey':secret}).raise_for_status().json()
        result=appliance.api.put('/state-protection/destination',json={'objectStorage':{'connectionId':created['connectionId'],'bucket':bucket,'prefix':'state'},'acknowledgedBasicClaimSafety':False,'acknowledgedThirdPartyConnector':False})
        result.raise_for_status();appliance.api.wait_operation(result.json()['id'])
        owner=json.loads(client.get_object(Bucket=bucket,Key='state/_filedock/owner.json')['Body'].read());assert owner['generation']==1
        wait(lambda:appliance.api.get('/state-protection').raise_for_status().json()['machineLossCovered'],timeout=60)
        index=next(o for o in client.list_objects_v2(Bucket=bucket,Prefix='state/appliance/')['Contents'] if o['Key'].endswith('/index.json'))
        assert json.loads(client.get_object(Bucket=bucket,Key=index['Key'])['Body'].read())['snapshots']
    finally:
        appliance.api.close()
        objects=client.list_objects_v2(Bucket=bucket).get('Contents',[])
        if objects:client.delete_objects(Bucket=bucket,Delete={'Objects':[{'Key':o['Key']} for o in objects]})
        client.delete_bucket(Bucket=bucket)
