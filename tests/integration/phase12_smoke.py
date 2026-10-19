#!/usr/bin/env python3
"""Isolated real Core/Management route contract checks. No installed state is touched.
Build the Debug Core and Management binaries first. Provider I/O is checked separately
on the disposable VM with the Phase 12 fixtures; this test disables system services.
"""
import json,sqlite3,tempfile
from pathlib import Path
from phase6_7_smoke import Appliance
from phase11_smoke import request

def main():
    with tempfile.TemporaryDirectory(prefix='fd-p12-',dir='/tmp') as directory:
        root=Path(directory);a=Appliance(root/'a',root/'copy')
        try:
            a.setup()
            code,connectors=request(a,'GET','/connectors');assert code==200,(code,connectors)
            assert {'fd-s3','rclone'} <= {c['subsystemId'] for c in connectors['connectors']}
            code,table=request(a,'GET','/connector-routes');assert code==200,(code,table)
            backup=next(r for r in table['defaults'] if r['family']=='s3-compatible' and r['use']=='BackupRepository');assert backup['target']['backupEngine']
            body={'name':'phase12','providerFamily':'s3-compatible','definition':{'type':'s3','endpoint':'https://example.invalid','region':'us-east-1','addressingStyle':'Path','bucket':'test','accessKey':'P12-SYNTHETIC-ACCESS','secretKey':'P12-SYNTHETIC-SECRET'}}
            code,c=request(a,'POST','/connections',body);assert code==201,(code,c)
            changes=[{'family':'s3-compatible','use':'OwnedObjectStorage','connectionId':'','target':{'connectorId':'rclone'}}]
            payload={'changes':changes,'expectedGeneration':table['generation']}
            code,preview=request(a,'POST','/connector-routes/preview',payload);assert code==200,(code,preview)
            assert preview['requiresBasicClaimAck'] and c['id'] in preview['connectionIds']
            assert request(a,'GET','/connector-routes')[1]==table,'Preview mutated routes'
            code,error=request(a,'PUT','/connector-routes',payload);assert code==422 and 'route.ack_required' in error['detail'],(code,error)
            assert request(a,'GET','/connector-routes')[1]==table,'Rejected apply mutated routes'
            code,changed=request(a,'PUT','/connector-routes',{**payload,'acknowledgedBasicClaimSafety':True});assert code==200,(code,changed)
            assert changed['generation']==table['generation']+1
            assert request(a,'PUT','/connector-routes',{**payload,'acknowledgedBasicClaimSafety':True})[0]==409
            code,error=request(a,'PATCH','/connectors/rclone',{'enabled':False});assert code==409 and error['detail']=='connector.in_use' and error['routes'],(code,error)
            override={'changes':[{'family':'s3-compatible','use':'OwnedObjectStorage','connectionId':c['id'],'target':{'connectorId':'fd-s3'}}],'expectedGeneration':changed['generation'],'acknowledgedBasicClaimSafety':True}
            code,changed=request(a,'PUT','/connector-routes',override);assert code==200,(code,changed)
            assert any(r['connectionId']==c['id'] for r in changed['overrides'])
            current=request(a,'GET','/connections')[1]['items'][0];assert current['effectiveConnectors']['OwnedObjectStorage']=='fd-s3' and current['spec']['routeOverrides']
            # Generic CRUD cannot bypass route acknowledgements.
            code,error=request(a,'PATCH','/connections/'+c['id'],{**current['spec'],'routeOverrides':{}},current['generation']);assert code==422 and error['detail']=='route.use_routing_api',(code,error)
            # Incompatible target and invalid use are preview issues; no apply side effects.
            invalid={'changes':[{'family':'s3-compatible','use':'CloudMount','target':{'connectorId':'fd-s3'}}],'expectedGeneration':changed['generation']}
            code,value=request(a,'POST','/connector-routes/preview',invalid);assert code==200 and any(i['code']=='route.connector_incapable' for i in value['issues']),(code,value)
            assert request(a,'PUT','/connector-routes',invalid)[0]==422
            for scope in ['ReadOnly','Full']:
                code,token=request(a,'POST','/tokens',{'name':'phase12-'+scope,'scope':scope});assert code==201
                assert request(a,'GET','/connector-routes',token=token['token'])[0]==200
                if scope=='ReadOnly':assert request(a,'POST','/connector-routes/preview',payload,token=token['token'])[0]==403
            # Acknowledgements link to an actual Management audit entry.
            with sqlite3.connect(root/'a/core.db') as db:
                rows=db.execute("SELECT spec FROM resource WHERE kind='ConnectorRoutes'").fetchall()
                assert rows
                acknowledgements=json.loads(rows[0][0])['acknowledgements']
            audit=request(a,'GET','/audit?pageSize=100')[1]['items'];ids={r['id'] for r in audit}
            assert all(v['auditEntryId'] in ids for v in acknowledgements)
            print('PASS: Phase12 route preview/apply, generation, overrides, scope guards, in-use conflicts and audited acknowledgements')
        finally:a.stop()
if __name__=='__main__':main()
