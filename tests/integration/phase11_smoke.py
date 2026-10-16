#!/usr/bin/env python3
"""Real temporary Core + Management API regression. No host storage/services are modified.
Linux provisioning, reserve fill, shares and Docker I/O are separate disposable-VM acceptance.
"""
import json, tempfile, urllib.request, urllib.error, http.cookies
from pathlib import Path
from phase6_7_smoke import Appliance

def request(a,method,path,body=None,generation=None,token=None):
    headers={'Content-Type':'application/json','Cookie':'; '.join(f'{k}={v}' for k,v in a.cookies.items()),'X-CSRF-Token':a.cookies.get('fd_http_csrf','')}
    if token is not None: headers={'Content-Type':'application/json','Authorization':'Bearer '+token}
    if generation is not None: headers['If-Match']=str(generation)
    req=urllib.request.Request(f'http://127.0.0.1:{a.port}/api/v1'+path,data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
    try: response=urllib.request.urlopen(req,timeout=30)
    except urllib.error.HTTPError as e: response=e
    data=response.read();return response.status,json.loads(data) if data else None

def main():
    with tempfile.TemporaryDirectory(prefix='fd-p11-',dir='/tmp') as directory:
        root=Path(directory);a=Appliance(root/'a',root/'copy')
        try:
            a.setup()
            for path in ['/pools','/share-users','/connections','/apps']:
                code,page=request(a,'GET',path);assert code==200 and page['items']==[],(path,code,page)
            # Runtime disabled means no destructive operation can execute on the development host.
            code,value=request(a,'POST','/pools',{'name':'media','diskSerials':['DATA'],'typedConfirmation':'ERASE DATA'})
            assert code==422 and 'storage.runtime_unavailable' in json.dumps(value),(code,value)
            assert request(a,'GET','/pools')[1]['items']==[]
            code,user=request(a,'POST','/share-users',{'username':'p11share','password':'FD-P11-SHARE-CANARY','enabled':True});assert code==201,(code,user)
            assert 'FD-P11-SHARE-CANARY' not in json.dumps(user)
            original=user['spec']['passwordSecret']
            body={'username':'p11share','password':'FD-P11-REPLACEMENT','enabled':True}
            assert request(a,'PATCH','/share-users/'+user['id'],body)[0]==428
            code,changed=request(a,'PATCH','/share-users/'+user['id'],body,user['generation']);assert code==200,(code,changed)
            assert changed['spec']['passwordSecret']!=original
            assert request(a,'PATCH','/share-users/'+user['id'],{'username':'p11share','password':'FD-P11-STALE'},user['generation'])[0]==409
            current=request(a,'GET','/share-users')[1]['items'][0];assert current['spec']['passwordSecret']==changed['spec']['passwordSecret']
            assert request(a,'DELETE','/share-users/'+user['id'],generation=changed['generation'])[0]==204
            config={'name':'drive','providerFamily':'google-drive','definition':{'type':'bound','connectorId':'rclone','config':{'scope':'drive'},'credentials':[]}}
            code,connection=request(a,'POST','/connections',config);assert code==201,(code,connection)
            secret_config={**config,'name':'bad','definition':{**config['definition'],'config':{'token':'NO-PUBLIC-TOKEN'}}}
            code,problem=request(a,'POST','/connections',secret_config);assert code==422,(code,problem)
            assert 'NO-PUBLIC-TOKEN' not in json.dumps(problem)
            assert request(a,'DELETE','/connections/'+connection['id'],generation=connection['generation'])[0]==204
            assert request(a,'POST','/connections',{**config,'providerFamily':'x-missing-family'})[0]==422
            s3={'name':'s3','providerFamily':'s3-compatible','definition':{'type':'s3','endpoint':'https://example.invalid','region':'us-east-1','addressingStyle':'Path','bucket':'test','accessKey':'FD-P11-ACCESS','secretKey':'FD-P11-SECRET'}}
            code,connection=request(a,'POST','/connections',s3);assert code==201,(code,connection)
            assert not any(v in json.dumps(connection) for v in ['FD-P11-ACCESS','FD-P11-SECRET'])
            assert request(a,'DELETE','/connections/'+connection['id'],generation=connection['generation'])[0]==204
            body={'name':'example','compose':'services:\n  example:\n    image: example.invalid/test:1\n','environment':{},'secrets':[]}
            code,app=request(a,'POST','/apps',body);assert code==201,(code,app)
            assert request(a,'PATCH','/apps/'+app['id'],body)[0]==428
            code,changed=request(a,'PATCH','/apps/'+app['id'],body,app['generation']);assert code==200,(code,changed)
            assert request(a,'PATCH','/apps/'+app['id'],body,app['generation'])[0]==409
            code,problem=request(a,'PATCH','/apps/'+app['id'],{**body,'name':'renamed'},changed['generation']);assert code==422,(code,problem)
            assert request(a,'POST','/apps/'+app['id']+'/stop')[0]==422
            assert request(a,'DELETE','/apps/'+app['id'],generation=changed['generation'])[0]==204
            code,settings=request(a,'GET','/system/storage-settings');assert code==200,(code,settings)
            assert request(a,'PUT','/system/storage-settings',{'warningPercent':80,'criticalPercent':70},settings['generation'])[0]==422
            assert request(a,'PUT','/system/storage-settings',{'warningPercent':85,'criticalPercent':95})[0]==428
            code,value=request(a,'PUT','/system/storage-settings',{'warningPercent':85,'criticalPercent':95},settings['generation']);assert code==200,(code,value,settings)
            assert request(a,'GET','/system/storage-settings')[1]['warningPercent']==85
            # Read-only tokens cannot mutate; full tokens cannot change session-only limits/settings.
            for scope in ['ReadOnly','Full']:
                code,issued=request(a,'POST','/tokens',{'name':'phase11-'+scope,'scope':scope});assert code==201,(code,issued)
                token=issued['token']
                for path in ['/pools','/share-users','/connections','/apps']:
                    code,value=request(a,'GET',path,token=token);assert code==200,(scope,path,code,value)
                assert request(a,'PUT','/system/storage-settings',{'warningPercent':80,'criticalPercent':95},1,token)[0]==403
                assert request(a,'PUT','/pools/system/limits',{'maxUsagePercent':50,'reservedFreeGb':100},1,token)[0]==403
                if scope=='ReadOnly':
                    for path in ['/apps','/pools','/share-users','/connections','/connections/oauth/start']:
                        assert request(a,'POST',path,{},token=token)[0]==403
            # Cursor pages have no duplicated resource.
            for i in range(3):assert request(a,'POST','/apps',{**body,'name':f'page-{i}'})[0]==201
            first=request(a,'GET','/apps?pageSize=2')[1];assert len(first['items'])==2 and first['nextPageToken']
            second=request(a,'GET','/apps?pageSize=2&pageToken='+first['nextPageToken'])[1]
            assert len(second['items'])==1 and not ({r['id'] for r in first['items']}&{r['id'] for r in second['items']})
            for p in [root/'a/secrets.enc',root/'a/core.db',root/'a/logs']:
                data=p.read_bytes();assert not any(s.encode() in data for s in ['FD-P11-SECRET','FD-P11-ACCESS','FD-P11-SHARE-CANARY'])
            print('PASS: Phase11 real API CRUD, concurrency, encrypted credentials, provider/schema guards, pagination, runtime isolation and pressure settings')
        finally:a.stop()
if __name__=='__main__':main()
