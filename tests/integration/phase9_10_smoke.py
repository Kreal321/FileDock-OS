#!/usr/bin/env python3
"""Real temporary Core + Management API smoke. Does not claim Linux mount/notification acceptance."""
import http.cookies,io,json,subprocess,tarfile,tempfile,urllib.request,uuid,os,time
from phase6_7_smoke import REPO,wait
from pathlib import Path
from phase6_7_smoke import Appliance

def main():
 with tempfile.TemporaryDirectory(prefix='fd-p910-',dir='/tmp') as d:
  root=Path(d)
  # A journalctl process fixture makes HTTP log filters deterministic on non-systemd hosts.
  bindir=root/'bin';bindir.mkdir();journal=bindir/'journalctl'
  journal.write_text('#!/usr/bin/env python3\nimport json,sys,datetime\nnow=datetime.datetime.now(datetime.timezone.utc)\nrows=[]\nfor cursor,age,unit,priority,message in [("c4",15,"filedock-management.service","4","management warning"),("c3",30,"filedock-core.service","2","disk critical"),("c2",60,"filedock-core.service","6","disk info"),("c1",90,"filedock-core.service","4","disk warning")]:\n rows.append({"__CURSOR":cursor,"__REALTIME_TIMESTAMP":str(int(now.timestamp()*1000000-age*1000000)),"_SYSTEMD_UNIT":unit,"PRIORITY":priority,"MESSAGE":message})\ncursor=next((a.split("=",1)[1] for a in sys.argv if a.startswith("--cursor=")),None)\nif cursor:rows=rows[next((i for i,r in enumerate(rows) if r["__CURSOR"]==cursor),len(rows)):]\nfor row in rows:print(json.dumps(row))\n')
  journal.chmod(0o755);original_path=os.environ['PATH'];os.environ['PATH']=str(bindir)+os.pathsep+original_path
  try:a=Appliance(root/'appliance',root/'copy')
  finally:os.environ['PATH']=original_path
  try:
   a.setup()
   assert a.call('GET','/api/v1/activity')[0]==200
   code,logs=a.call('GET','/api/v1/logs?source=filedock-core.service&minSeverity=Warning&text=disk&pageSize=1');assert code==200 and len(logs['items'])==1 and logs['items'][0]['severity']=='Critical',logs
   code,next_logs=a.call('GET','/api/v1/logs?source=filedock-core.service&minSeverity=Warning&text=disk&pageSize=1&pageToken='+logs['nextPageToken']);assert code==200 and next_logs['items'][0]['severity']=='Warning',next_logs
   assert logs['items'][0]['at']>next_logs['items'][0]['at']
   import datetime,urllib.parse
   since=(datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(seconds=45)).isoformat()
   code,recent=a.call('GET','/api/v1/logs?source=filedock-core.service&minSeverity=Warning&since='+urllib.parse.quote(since));assert code==200 and len(recent['items'])==1 and recent['items'][0]['severity']=='Critical',recent


   assert a.call('GET','/api/v1/alerts')[0]==200
   assert a.call('GET','/api/v1/notification-channels')[1]['items']==[]
   assert a.call('GET','/api/v1/system/status')[1]['notificationChannelMissing']
   options=a.call('GET','/api/v1/folders/options');assert options[0]==200
   modes={m['id']:m for m in options[1]['modes']};assert modes['LocalOnly']['enabled'] and not modes['HybridBackup']['enabled'] and not modes['TieredStorage']['enabled']
   # An isolated desired-state fixture supplies a pool identity, not a real disk or mount.
   import sqlite3,datetime
   pool=str(uuid.uuid4());db=sqlite3.connect(root/'appliance'/'core.db');now=datetime.datetime.now(datetime.timezone.utc).isoformat()
   db.execute("INSERT INTO resource(kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at) VALUES('Pool',?,1,?,NULL,0,1,?,?)",(pool,json.dumps({'name':'test-data','kind':'Adopted','memberDisks':[{'serial':'test-only'}]}),now,now));db.commit();db.close()
   body={'identity':{'name':'Movies','fileDockPath':'/storage/movies'},'storageMode':'LocalOnly','reference':{'poolId':pool,'relativePath':'/movies'},'dataImportance':None,'access':[],'advanced':{}}
   code,report=a.call('POST','/api/v1/folders/validate',body);assert code==200 and report['valid'],report
   code,folder=a.call('POST','/api/v1/folders',body);assert code==201,(code,folder)
   # Paths are generated from the name; a supplied custom path cannot bypass overlap checks.
   duplicate=dict(body,identity={'name':'Movies','fileDockPath':'/storage/unrelated'})
   code,report=a.call('POST','/api/v1/folders/validate',duplicate);assert code==200 and not report['valid'] and any(i['code']=='folder.path_overlap' for i in report['issues'])
   bad=dict(body,storageMode='HybridBackup',dataImportance='Critical');bad['reference']={**body['reference'],'backupDestination':{'connectionId':str(uuid.uuid4()),'remotePath':'backup'}}
   code,report=a.call('POST','/api/v1/folders/validate',bad);assert code==200 and any(i['code']=='mode.capability_missing' for i in report['issues'])
   def request(method,path,body=None,generation=None):
    headers={'Cookie':'; '.join(f'{k}={v}' for k,v in a.cookies.items()),'X-CSRF-Token':a.cookies.get('fd_http_csrf',''),'Content-Type':'application/json'}
    if generation is not None:headers['If-Match']=str(generation)
    req=urllib.request.Request(f'http://127.0.0.1:{a.port}/api/v1'+path,data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
    try:r=urllib.request.urlopen(req,timeout=30)
    except __import__('urllib.error',fromlist=['HTTPError']).HTTPError as e:r=e
    raw=r.read();return r.status,json.loads(raw) if raw else None
   assert request('PATCH','/folders/'+folder['id'],body)[0]==428
   changed={**body,'identity':{'name':'Movies renamed','fileDockPath':'/storage/movies'}}
   code,updated=request('PATCH','/folders/'+folder['id'],changed,folder['generation']);assert code==200,(code,updated)
   assert request('PATCH','/folders/'+folder['id'],changed,folder['generation'])[0]==409
   moved={**changed,'reference':{'poolId':pool,'relativePath':'/moved'}}
   code,report=request('PATCH','/folders/'+folder['id'],moved,updated['generation']);assert code==422 and any(i['code']=='folder.reference_immutable' for i in report['issues'])
   # Exercise declarative exports atomically; this does not apply real share rules.
   db=sqlite3.connect(root/'appliance'/'core.db');db.execute("UPDATE subsystem SET state='Running' WHERE id='samba'");db.commit();db.close()
   shared={'identity':{'name':'Shared','fileDockPath':'/storage/shared'},'storageMode':'LocalOnly','reference':{'poolId':pool,'relativePath':'/shared'},'dataImportance':None,'access':['Smb'],'advanced':{}}
   code,shared_folder=a.call('POST','/api/v1/folders',shared);assert code==201,(code,shared_folder)
   assert request('DELETE','/folders/'+shared_folder['id'],generation=shared_folder['generation'])[0]==422
   shared['access']=[]
   code,without_share=request('PATCH','/folders/'+shared_folder['id'],shared,shared_folder['generation']);assert code==200,(code,without_share)
   assert request('DELETE','/folders/'+shared_folder['id'],generation=without_share['generation'])[0]==204
   # A registry fixture enables policy validation only, without implementing or claiming a backup data plane.
   db=sqlite3.connect(root/'appliance'/'core.db');provider={'id':'policy-test','name':'Policy test provider','capabilities':['backup.snapshot','backup.restore','backup.retention'],'runtime':{'type':'native-process'}}
   db.execute("INSERT INTO subsystem(id,class,manifest,version,approved_permissions,signature,state,enabled,data_bindings,updated_at) VALUES('policy-test','BuiltIn',?,'1.0.0','[]','{}','Running',1,'{}',?)",(json.dumps(provider),now));db.commit();db.close()
   critical={'identity':{'name':'Policy','fileDockPath':'/storage/policy'},'storageMode':'HybridBackup','reference':{'poolId':pool,'relativePath':'/policy','backupDestination':{'poolId':pool,'relativePath':'/backups'}},'dataImportance':'Critical','access':[],'advanced':{}}
   code,policy=a.call('POST','/api/v1/folders',critical);assert code==201,(code,policy)
   assert policy['advanced']['backupIntervalSeconds']==300
   assert request('DELETE','/folders/'+policy['id'],generation=policy['generation'])[0]==422
   lower={**critical,'dataImportance':'Important'}
   assert request('PATCH','/folders/'+policy['id'],lower,policy['generation'])[0]==422
   lower['confirmImportanceReduction']=True
   code,lowered=request('PATCH','/folders/'+policy['id'],lower,policy['generation']);assert code==200,(code,lowered)
   assert lowered['advanced']['backupIntervalSeconds']==900
   assert request('DELETE','/folders/'+policy['id'],generation=lowered['generation'])[0]==204
   # The authenticated diagnostic path must mask arbitrary observed occurrences of vault values.
   canary='FD-DIAGNOSTIC-CANARY-'+uuid.uuid4().hex[:12]
   assert a.call('PUT','/api/v1/secrets/notification/diagnostic/key',{'value':canary,'consumers':[]})[0]==200
   db=sqlite3.connect(root/'appliance'/'core.db');spec={**body,'name':canary};spec.pop('identity');spec['filedockPath']='/storage/movies';db.execute("UPDATE resource SET spec=? WHERE kind='ManagedFolder' AND id=?",(json.dumps(spec),folder['id']));db.commit();db.close()
   req=urllib.request.Request(f'http://127.0.0.1:{a.port}/api/v1/diagnostics/bundle',headers={'Cookie':'; '.join(f'{k}={v}' for k,v in a.cookies.items())})
   with urllib.request.urlopen(req,timeout=30) as response: assert response.status==200;archive=response.read()
   decoded=subprocess.run(['zstd','-q','-d','-c'],input=archive,capture_output=True,check=True).stdout
   assert canary.encode() not in decoded
   with tarfile.open(fileobj=io.BytesIO(decoded)) as tar: assert {'configuration.json','subsystem-health.json','services.json','logs.json'}==set(tar.getnames())
   audit=a.call('GET','/api/v1/audit?action=diagnostics.exported');assert audit[0]==200 and audit[1]['items']
   assert a.call('GET','/api/v1/logs?source=../../etc/shadow')[0]==400
   # Register a real out-of-process gRPC recorder without claiming OS sandbox coverage.
   manifest='schemaVersion: 1\nid: notify-test\nname: Protocol recorder\nversion: 1.0.0\nclass: extension\ncapabilities: [notify.channel]\nruntime: {type: native-process, executable: ./sample}\nhealth: {type: grpc}\npermissions: [core.alerts.write]\ncompatibility: {appliance: ">=0.1.0 <0.2.0"}\n'
   payload=root/'appliance'/'extensions'/'runtimes'/('a'*64);payload.mkdir(parents=True);(payload/'manifest.yaml').write_text(manifest)
   db=sqlite3.connect(root/'appliance'/'core.db')
   spec={'subsystemId':'notify-test','version':'1.0.0','packageSha256':'a'*64,'config':{},'approvedPermissions':['core.alerts.write'],'signature':{'unsigned':{'acknowledged':True}},'dataBindings':{}}
   registry={'schemaVersion':1,'id':'notify-test','name':'Protocol recorder','version':'1.0.0','class':'extension','capabilities':['notify.channel'],'runtime':{'type':'native-process','executable':'./sample'},'health':{'type':'grpc'},'permissions':['core.alerts.write'],'compatibility':{'appliance':'>=0.1.0 <0.2.0'}}
   db.execute("INSERT INTO resource(kind,id,generation,spec,status,deleted,change_seq,created_at,updated_at) VALUES('Extension',?,1,?,NULL,0,1,?,?)",(str(uuid.uuid4()),json.dumps(spec),now,now))
   db.execute("INSERT INTO subsystem(id,class,manifest,version,approved_permissions,signature,state,enabled,package_sha256,data_bindings,updated_at) VALUES('notify-test','Extension',?,'1.0.0',?,?,'Running',1,?,'{}',?)",(json.dumps(registry),json.dumps(spec['approvedPermissions']),json.dumps(spec['signature']),'a'*64,now))
   db.execute("INSERT OR REPLACE INTO setting(key,value,updated_at) VALUES(?,?,?)",('subsystem.registered_capabilities.notify-test','["notify.channel"]',now));db.commit();db.close()
   code,channel=a.call('POST','/api/v1/notification-channels',{'name':'Recorder','subsystemId':'notify-test','minSeverity':'Warning','config':{},'secretRefs':[]});assert code==200,(code,channel)
   envfile=root/'appliance'/'extension-env'/'notify-test.env';env=dict(os.environ)
   for line in envfile.read_text().splitlines():
    k,v=line.split('=',1);env[k]=v.strip('"')
   bridge=Path(env['FD_CORE_SOCKET']);bridge.parent.mkdir(parents=True,exist_ok=True);bridge.symlink_to(root/'appliance'/'extensions.sock')
   recording=root/'deliveries';recording.mkdir();env['FD_SAMPLE_DELIVERIES']=str(recording)
   binary=REPO/'target/debug/fd-extension-sample';assert binary.is_file(),'Build fd-extension-sample before this smoke'
   recorder=subprocess.Popen([str(binary),'--protocol-test-only'],env=env,stdout=a.logs,stderr=a.logs);a.children.append(recorder)
   wait(lambda:Path(env['FD_EXTENSION_SOCKET']).exists())
   code,body=a.call('POST','/api/v1/notification-channels/'+channel['id']+'/test',{});assert code==200,(code,body)
   subprocess.run([str(binary),'--raise-test-alert','--protocol-test-only'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True)
   file=recording/'deliveries.jsonl'
   wait(lambda:file.exists() and 'Critical' in file.read_text())
   rows=[json.loads(line) for line in file.read_text().splitlines()];assert len({r['attemptKey'] for r in rows})==len(rows)
   critical=next(row for row in a.call('GET','/api/v1/alerts?state=Active&minSeverity=Critical')[1]['items'] if row['kind']=='phase9.test_critical')
   assert a.call('POST','/api/v1/alerts/'+critical['id']+'/ack',{})[0]==204
   wait(lambda:'acknowledged' in file.read_text())
   assert not a.call('GET','/api/v1/system/status')[1]['notificationChannelMissing']
   print('PASS real out-of-process notify registration, send-test, critical delivery, deduplication and acknowledgement transition')
   print('PASS activity/alerts/channels, mode gating, folder dry-run/create/overlap, redacted diagnostic download and export audit')
  finally:a.stop()
if __name__=='__main__':main()
