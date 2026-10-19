"""US10 acceptance. Use disposable VMs and trusted TLS test storage only.

FD_E2E_STATE_ENDPOINT/ACCESS_KEY/SECRET_KEY select MinIO. For the proxy check also
set FD_E2E_S3_PROXY_ENDPOINT and FD_E2E_S3_PROXY_CONTROL (trusted TLS URLs).
FD_E2E_CONNECTOR_PACKAGE points to the bundled sample-connector.fdext.
Missing external fixtures are explicit skips, never successful acceptance evidence.
"""
import concurrent.futures
import json
import os
from pathlib import Path
import uuid
import boto3
import httpx
import pytest
from state_helpers import setup, snapshot, wait

def route(api, connector, connection_id='', acknowledge=True):
    table=api.get('/connector-routes').raise_for_status().json()
    change={'family':'s3-compatible','use':'OwnedObjectStorage','connectionId':connection_id,'target':None if connector is None else {'connectorId':connector}}
    body={'changes':[change],'expectedGeneration':table['generation'],'acknowledgedBasicClaimSafety':acknowledge,'acknowledgedThirdPartyOwnedStorage':acknowledge}
    return api.put('/connector-routes',json=body)

def connection(api,endpoint,bucket,access,secret,name):
    return api.post('/connections',json={'name':name,'providerFamily':'s3-compatible','definition':{'type':'s3','endpoint':endpoint,'region':'us-east-1','bucket':bucket,'addressingStyle':'Path','accessKey':access,'secretKey':secret}}).raise_for_status().json()

def destination(api,connection_id,bucket,prefix='state',ack=False):
    response=api.put('/state-protection/destination',json={'objectStorage':{'connectionId':connection_id,'bucket':bucket,'prefix':prefix},'acknowledgedBasicClaimSafety':ack,'acknowledgedThirdPartyConnector':ack})
    if response.is_success:assert api.wait_operation(response.json()['id'])['state']=='Succeeded'
    return response

@pytest.fixture
def cloud():
    endpoint=os.environ.get('FD_E2E_STATE_ENDPOINT','')
    if not endpoint.startswith('https://'):pytest.skip('Trusted TLS MinIO fixture is required for live connector acceptance.')
    access=os.environ['FD_E2E_STATE_ACCESS_KEY'];secret=os.environ['FD_E2E_STATE_SECRET_KEY']
    client=boto3.client('s3',endpoint_url=endpoint,aws_access_key_id=access,aws_secret_access_key=secret,region_name='us-east-1')
    bucket='fd-routing-'+uuid.uuid4().hex[:12];client.create_bucket(Bucket=bucket)
    try:yield endpoint,access,secret,client,bucket
    finally:
        for page in client.get_paginator('list_objects_v2').paginate(Bucket=bucket):
            objects=[{'Key':o['Key']} for o in page.get('Contents',[])]
            if objects:client.delete_objects(Bucket=bucket,Delete={'Objects':objects})
        client.delete_bucket(Bucket=bucket)

def objects(client,bucket):
    return {o['Key']:o['ETag'] for page in client.get_paginator('list_objects_v2').paginate(Bucket=bucket) for o in page.get('Contents',[])}

def test_route_contracts_and_per_connection_override(installed_vm):
    appliance=setup(installed_vm);api=appliance.api
    try:
        table=api.get('/connector-routes').raise_for_status().json()
        assert next(r for r in table['defaults'] if r['family']=='s3-compatible' and r['use']=='BackupRepository')['target']['backupEngine']
        assert {'fd-s3','rclone'} <= {c['subsystemId'] for c in api.get('/connectors').raise_for_status().json()['connectors']}
        c=connection(api,'https://example.invalid','test','synthetic-access','synthetic-secret','override-only')
        assert route(api,'rclone',c['id'],False).status_code==422
        changed=route(api,'rclone',c['id']).raise_for_status().json();assert changed['generation']==table['generation']+1
        assert route(api,'fd-s3').is_success
        row=next(r for r in api.get('/connections').raise_for_status().json()['items'] if r['id']==c['id'])
        assert row['effectiveConnectors']['OwnedObjectStorage']=='rclone'
        refused=api.patch('/connectors/rclone',json={'enabled':False});assert refused.status_code==409 and refused.json()['routes']
        assert route(api,None,c['id']).is_success
    finally:api.close()

def test_ten_switches_during_twenty_snapshots(installed_vm,cloud):
    endpoint,access,secret,client,bucket=cloud;appliance=setup(installed_vm);api=appliance.api
    try:
        c=connection(api,endpoint,bucket,access,secret,'route-switch')
        probe=api.post('/connections/'+c['id']+'/test',json={}).raise_for_status().json()
        assert probe['connectorId']=='fd-s3' and probe['conditionalCreate'] and probe['conditionalReplace']
        destination(api,c['id'],bucket).raise_for_status();snapshot(appliance)
        initial=objects(client,bucket)
        immutable={k:v for k,v in initial.items() if k.endswith(('.fdstate','.fdpkg'))}
        def copies():
            for _ in range(20):snapshot(appliance)
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(copies)
            for i in range(10):
                target='rclone' if i%2==0 else 'fd-s3'
                table=api.get('/connector-routes').raise_for_status().json()
                payload={'changes':[{'family':'s3-compatible','use':'OwnedObjectStorage','target':{'connectorId':target}}],'expectedGeneration':table['generation']}
                preview=api.post('/connector-routes/preview',json=payload).raise_for_status().json()
                assert c['id'] in preview['connectionIds']
                if target=='rclone':
                    assert preview['requiresBasicClaimAck'] and 'conditional_writes' in preview['lostFeatures']
                    assert api.put('/connector-routes',json=payload).status_code==422
                api.put('/connector-routes',json={**payload,'acknowledgedBasicClaimSafety':True}).raise_for_status()
            pending.result(timeout=600)
        final=objects(client,bucket)
        assert all(final.get(k)==v for k,v in immutable.items()),'An existing immutable object changed or disappeared'
        assert sum(k.endswith('.fdstate') for k in final)>=20
        # Native and rclone both read the same protected destination during switches.
        assert api.post('/connections/'+c['id']+'/test',json={}).raise_for_status().json()['connectorId']=='fd-s3'
    finally:api.close()

def test_foreign_root_probe_does_not_write(installed_vm,cloud):
    endpoint,access,secret,client,bucket=cloud;appliance=setup(installed_vm);api=appliance.api
    try:
        c=connection(api,endpoint,bucket,access,secret,'foreign-root')
        owner={'schemaVersion':1,'applianceId':str(uuid.uuid4()),'applianceName':'other','generation':1,'purpose':'ApplianceState','claimedAt':'2026-01-01T00:00:00Z','lastWriteAt':'2026-01-01T00:00:00Z'}
        client.put_object(Bucket=bucket,Key='_filedock/owner.json',Body=json.dumps(owner).encode())
        before=objects(client,bucket)
        probe=api.post('/connections/'+c['id']+'/test',json={}).raise_for_status().json()
        assert not probe.get('writeStepsRun',False) and not probe.get('conditionalCreate',False)
        assert objects(client,bucket)==before
    finally:api.close()

def test_basic_proxy_and_corrupted_upload_never_succeed(installed_vm,cloud):
    proxy=os.environ.get('FD_E2E_S3_PROXY_ENDPOINT','');control=os.environ.get('FD_E2E_S3_PROXY_CONTROL','')
    if not proxy.startswith('https://') or not control.startswith('https://'):pytest.skip('Trusted TLS fault proxy and control URLs are required.')
    _,access,secret,_,bucket=cloud;appliance=setup(installed_vm);api=appliance.api
    try:
        c=connection(api,proxy,bucket,access,secret,'basic-provider')
        probe=api.post('/connections/'+c['id']+'/test',json={}).raise_for_status().json();assert not probe.get('conditionalCreate',False)
        assert destination(api,c['id'],bucket,ack=False).status_code==422
        destination(api,c['id'],bucket,ack=True).raise_for_status()
        assert api.get('/system/status').raise_for_status().json()['basicClaimSafetyWarning']
        for fault in ['flip','truncate']:
            httpx.post(control,json={'upload':fault,'download':'none'}).raise_for_status()
            response=api.post('/connections/'+c['id']+'/test',json={})
            assert not response.is_success or any(not check['ok'] for check in response.json()['checks'])
    finally:
        httpx.post(control,json={'upload':'none','download':'none'}).raise_for_status();api.close()

def test_third_party_install_changes_no_routes_and_needs_two_acknowledgements(installed_vm,cloud):
    path=Path(os.environ.get('FD_E2E_CONNECTOR_PACKAGE','dist/test-fixtures/phase12/sample-connector.fdext'))
    if not path.is_file():pytest.skip('Build the bundled sample connector for the target architecture first.')
    endpoint,access,secret,_,bucket=cloud;appliance=setup(installed_vm);api=appliance.api
    try:
        c=connection(api,endpoint,bucket,access,secret,'third-party')
        destination(api,c['id'],bucket,ack=True).raise_for_status()
        before=api.get('/connector-routes').raise_for_status().json()
        with path.open('rb') as file:
            inspected=api.http.post('/extensions/inspect',files={'file':(path.name,file,'application/octet-stream')},headers=api._csrf()).raise_for_status().json()
        assert inspected['valid'] and 'cloud.credentials' in inspected['requestedPermissions']
        operation=api.post('/extensions',json={'stagedPackageId':inspected['stagedPackageId'],'approvedPermissions':inspected['requestedPermissions'],'acknowledgedUnsigned':True,'dataBindings':{}}).raise_for_status().json()
        assert api.wait_operation(operation['id'])['state']=='Succeeded'
        assert api.get('/connector-routes').raise_for_status().json()==before
        assert route(api,'sample-connector',c['id'],False).status_code==422
        route(api,'sample-connector',c['id']).raise_for_status()
        snapshot(appliance)
        locations=api.get('/locations').raise_for_status().json()['locations']
        assert any(l['servedByConnector']=='sample-connector' and l['servedByThirdParty'] for l in locations)
    finally:api.close()

def test_stopped_routed_connector_fails_without_fallback(installed_vm,cloud):
    endpoint,access,secret,_,bucket=cloud;appliance=setup(installed_vm);api=appliance.api
    try:
        c=connection(api,endpoint,bucket,access,secret,'no-fallback');destination(api,c['id'],bucket,ack=True).raise_for_status()
        installed_vm.sudo('systemctl stop filedock-s3.service; systemctl mask --runtime filedock-s3.service')
        wait(lambda:any(c['subsystemId']=='fd-s3' and c['state']!='Running' for c in api.get('/connectors').raise_for_status().json()['connectors']))
        response=api.post('/connections/'+c['id']+'/test',json={});assert response.status_code==503 and 'connector.unavailable' in response.text
        alerts=api.get('/alerts').raise_for_status().json()['items'];assert any(a['kind']=='connector.route_unavailable' and 'fd-s3' in a['message'] for a in alerts)
    finally:
        installed_vm.sudo('systemctl unmask --runtime filedock-s3.service; systemctl start filedock-s3.service');api.close()
