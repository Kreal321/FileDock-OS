"""SC-009: aggregate cgroup memory and CPU, normalized to ONE core.

Run idle: no active operations, browser polling or fixture jobs. A larger VM result
is useful evidence but does not establish the minimum-hardware acceptance gate.
"""
import json
import time

UNITS=('filedock-core.service','filedock-management.service')


def sample(vm):
    result={}
    for unit in UNITS:
        output=vm.sudo('systemctl show '+unit+' -p ActiveState -p MainPID -p MemoryCurrent -p CPUUsageNSec').stdout
        row=dict(line.split('=',1) for line in output.splitlines() if '=' in line)
        assert row['ActiveState']=='active',row
        result[unit]={key:int(row[key]) for key in ('MainPID','MemoryCurrent','CPUUsageNSec')}
    return result


def measure(vm,seconds=60,interval=5):
    first=sample(vm);start=time.monotonic();peak=sum(r['MemoryCurrent'] for r in first.values())
    while time.monotonic()-start<seconds:
        time.sleep(interval);last=sample(vm)
        assert all(last[u]['MainPID']==first[u]['MainPID'] for u in UNITS),'Service restarted during idle sample'
        peak=max(peak,sum(r['MemoryCurrent'] for r in last.values()))
    elapsed=time.monotonic()-start
    cpu=sum(last[u]['CPUUsageNSec']-first[u]['CPUUsageNSec'] for u in UNITS)/1e9/elapsed*100
    return {'seconds':elapsed,'peakCgroupBytes':peak,'cpuPercentOfOneCore':cpu}


def test_idle_services_fit_budget(minimum_installed_vm):
    installed_vm=minimum_installed_vm
    from state_helpers import setup
    appliance=setup(installed_vm)
    try:
        assert not appliance.api.get('/operations?activeOnly=true').raise_for_status().json()['items']
        time.sleep(30)
        result=measure(installed_vm)
        print(json.dumps(result))
        assert result['peakCgroupBytes']<512*1024*1024,result
        assert result['cpuPercentOfOneCore']<2,result
    finally:appliance.api.close()
