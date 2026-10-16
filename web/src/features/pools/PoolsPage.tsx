import { useResourcePage } from '../resources/useResourcePage.tsx';
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import { Panel } from '../../components/Panel.tsx';
import { PageHeader } from '../../components/PageHeader.tsx';
import { DataTable } from '../../components/DataTable.tsx';
import { Button } from '../../components/Button.tsx';
import { Dialog } from '../../components/Dialog.tsx';
import { Field } from '../../components/Field.tsx';
import { RowActions } from '../../components/RowActions.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { StateView } from '../../components/StateView.tsx';
import {
  useResourceAction,
  type Resource,
  type PageProps,
} from '../resources/useResourceAction.ts';
import { useUnsavedChanges } from '../../navigation/useUnsavedChanges.ts';
type Disk = {
  serial: string;
  devicePath: string;
  model: string;
  sizeBytes: number;
  isSystemDisk: boolean;
  isBlank: boolean;
  poolId: string;
  existingFilesystem: string;
  health: string;
};
type SystemPool = {
  generation: number;
  available: boolean;
  unavailableReason?: string;
  maxUsagePercent: number;
  reservedFreeBytes: string;
  effectiveCapacityBytes: string;
  usedBytes: string;
  limitingFactor: string;
  writable: string;
};
const gb = (bytes: unknown) => `${(Number(bytes ?? 0) / 1e9).toFixed(1)} GB`;
export function PoolsPage({ canMutate = true, connected = true }: PageProps) {
  const pagination = useResourcePage();
  const pools = useQuery({
    queryKey: ['pools', pagination.cursor],
    queryFn: () =>
      api.get<{ items: Resource[]; nextPageToken?: string | null }>(`/pools${pagination.query}`),
    refetchInterval: 10000,
  });
  const disks = useQuery({
    queryKey: ['disks'],
    queryFn: () => api.get<{ items: Disk[] }>('/disks'),
    refetchInterval: 30000,
  });
  const thresholds = useQuery({
    queryKey: ['storage-settings'],
    queryFn: () =>
      api.get<{ generation: number; warningPercent: number; criticalPercent: number }>(
        '/system/storage-settings',
      ),
  });
  const [warning, setWarning] = useState(90);
  const [critical, setCritical] = useState(95);
  const system = useQuery({
    queryKey: ['system-pool'],
    queryFn: () => api.get<SystemPool>('/pools/system'),
    refetchInterval: 10000,
  });
  const action = useResourceAction(canMutate, connected, [
    'pools',
    'disks',
    'system-pool',
    'storage-settings',
  ]);
  const [mode, setMode] = useState<
    '' | 'create' | 'adopt' | 'extend' | 'recreate' | 'limits' | 'thresholds'
  >('');
  const [pool, setPool] = useState<Resource | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [name, setName] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [percent, setPercent] = useState(50);
  const [reserve, setReserve] = useState(100);
  const [interruption, setInterruption] = useState(false);
  const dirty =
    !!mode &&
    (name !== '' ||
      selected.length > 0 ||
      confirmation !== '' ||
      ['limits', 'thresholds'].includes(mode));
  useUnsavedChanges(dirty);
  function open(next: typeof mode, row: Resource | null = null) {
    setMode(next);
    setPool(row);
    setSelected([]);
    setName('');
    setConfirmation('');
    setPercent(system.data?.maxUsagePercent ?? 50);
    setReserve(Number(system.data?.reservedFreeBytes ?? 100e9) / 1e9);
    setInterruption(false);
    setWarning(thresholds.data?.warningPercent ?? 90);
    setCritical(thresholds.data?.criticalPercent ?? 95);
    action.setError('');
  }
  function submit() {
    void action.act(
      () => {
        if (mode === 'thresholds')
          return api.put(
            '/system/storage-settings',
            { warningPercent: warning, criticalPercent: critical },
            String(thresholds.data?.generation),
          );
        if (mode === 'limits')
          return api.put(
            '/pools/system/limits',
            {
              maxUsagePercent: percent,
              reservedFreeGb: reserve,
              acknowledgedInterruption: interruption,
            },
            String(system.data?.generation),
          );
        if (mode === 'adopt') return api.post('/pools/adopt', { name, diskSerial: selected[0] });
        if (mode === 'extend')
          return api.post(`/pools/${pool?.id}/disks`, {
            diskSerial: selected[0],
            typedConfirmation: confirmation,
          });
        return api.post(mode === 'recreate' ? `/pools/${pool?.id}/recreate` : '/pools', {
          name,
          diskSerials: selected,
          typedConfirmation: confirmation,
        });
      },
      () => setMode(''),
    );
  }
  const eligible = (disks.data?.items ?? []).filter(
    (d) =>
      d.serial &&
      !d.isSystemDisk &&
      !d.poolId &&
      (mode === 'adopt' ? d.existingFilesystem === 'ext4' : d.isBlank),
  );
  const dataPools = pools.data?.items.filter((row) => row.spec.kind !== 'System') ?? [];
  const erase = `ERASE ${selected.join(' ')}`;
  return (
    <div className="stack resource-page">
      <PageHeader
        title="Pools & disks"
        description="Choose where your files live. Data pools combine disks without redundancy."
        action={
          <div className="row">
            <Button disabled={action.disabled} onClick={() => open('create')}>
              Create pool
            </Button>
            <Button variant="secondary" disabled={action.disabled} onClick={() => open('adopt')}>
              Adopt filesystem
            </Button>
          </div>
        }
      />
      {action.error && !mode ? <p role="alert">{action.error}</p> : null}
      {action.operation ? (
        <p role="status">
          <a href={`#/settings/operations/${action.operation}`}>View storage operation</a>
        </p>
      ) : null}
      <Panel className="stack">
        <div className="panel-heading">
          <h2>System Pool</h2>
          <Button
            variant="secondary"
            disabled={action.disabled || !system.data}
            onClick={() => open('limits')}
          >
            Change limits
          </Button>
        </div>
        {system.isPending ? (
          <p role="status">Loading System Pool…</p>
        ) : system.error ? (
          <p role="alert">{system.error.message}</p>
        ) : system.data ? (
          <>
            <StatusBadge state={system.data.available ? 'Ready' : 'Unavailable'} />
            <p>
              {system.data.available
                ? `${gb(system.data.usedBytes)} used · ${gb(system.data.effectiveCapacityBytes)} capacity`
                : system.data.unavailableReason}
            </p>
            <p className="muted">
              Maximum {system.data.maxUsagePercent}% of the system disk ·{' '}
              {gb(system.data.reservedFreeBytes)} kept free. {system.data.writable}
            </p>
            <p className="muted">Local Only files here have no off-appliance backup.</p>
          </>
        ) : null}
      </Panel>
      <Panel className="stack">
        <h2>Data pools</h2>
        <StateView
          phase={
            pools.isPending
              ? 'loading'
              : pools.error
                ? 'error'
                : dataPools.length
                  ? 'ready'
                  : 'empty'
          }
          error={pools.error?.message}
          onRetry={() => void pools.refetch()}
          empty="No data pools yet. Create one from blank data disks, or adopt an existing ext4 filesystem."
        />
        {dataPools.length ? (
          <DataTable
            label="Storage pools"
            headers={[
              'Pool',
              'Health',
              { id: 'capacity', label: 'Capacity', align: 'end' },
              { id: 'used', label: 'Used', align: 'end' },
              'Actions',
            ]}
          >
            {dataPools.map((r) => (
              <tr key={r.id}>
                <td>
                  <strong>{String(r.spec.name)}</strong>
                  <div className="caption muted">{String(r.spec.kind)}</div>
                </td>
                <td>
                  <StatusBadge state={String(r.status?.state ?? 'Pending')} />
                </td>
                <td>{gb(r.status?.capacityBytes)}</td>
                <td>{gb(r.status?.usedBytes)}</td>
                <td>
                  {r.spec.kind === 'Lvm' ? (
                    <RowActions
                      label={`Actions for ${r.spec.name}`}
                      actions={[
                        {
                          label: 'Add disk',
                          disabled: action.disabled,
                          onClick: () => open('extend', r),
                        },
                        {
                          label: 'Recreate failed pool',
                          danger: true,
                          disabled: action.disabled || r.status?.state !== 'Failed',
                          onClick: () => open('recreate', r),
                        },
                      ]}
                    />
                  ) : null}
                </td>
              </tr>
            ))}
          </DataTable>
        ) : null}
      </Panel>
      <Panel className="stack">
        <div className="panel-heading">
          <h2>Disks</h2>
          <Button
            variant="secondary"
            disabled={action.disabled || !thresholds.data}
            onClick={() => open('thresholds')}
          >
            Alert thresholds
          </Button>
        </div>
        <StateView
          phase={
            disks.isPending
              ? 'loading'
              : disks.error
                ? 'error'
                : disks.data?.items.length
                  ? 'ready'
                  : 'empty'
          }
          error={disks.error?.message}
          onRetry={() => void disks.refetch()}
          empty="No disks reported."
        />
        {disks.data?.items.length ? (
          <DataTable
            label="Disk inventory"
            headers={[
              'Disk',
              'Serial',
              { id: 'size', label: 'Size', align: 'end' },
              'Use',
              'Health',
            ]}
          >
            {disks.data.items.map((d) => (
              <tr key={d.devicePath}>
                <td>
                  <strong>{d.model || 'Disk'}</strong>
                  <div className="caption muted">{d.devicePath}</div>
                </td>
                <td>{d.serial || 'No stable serial — unavailable for pool creation'}</td>
                <td>{gb(d.sizeBytes)}</td>
                <td>
                  {d.isSystemDisk
                    ? 'System disk'
                    : d.poolId
                      ? 'Pool member'
                      : d.isBlank
                        ? 'Blank'
                        : d.existingFilesystem || 'Contains data'}
                </td>
                <td>{d.health}</td>
              </tr>
            ))}
          </DataTable>
        ) : null}
      </Panel>
      {pagination.controls(pools.data?.nextPageToken)}
      <Dialog
        open={!!mode}
        title={
          mode === 'thresholds'
            ? 'Disk pressure alerts'
            : mode === 'limits'
              ? 'System Pool limits'
              : mode === 'create'
                ? 'Create pool'
                : mode === 'adopt'
                  ? 'Adopt filesystem'
                  : mode === 'extend'
                    ? 'Add disk to pool'
                    : 'Recreate failed pool'
        }
        onClose={() => setMode('')}
        dirty={dirty}
        busy={action.busy}
        footer={
          <Button
            disabled={
              action.disabled ||
              (!['limits', 'thresholds'].includes(mode) &&
                (!selected.length ||
                  ((mode === 'create' || mode === 'adopt') && !name) ||
                  (mode !== 'adopt' && confirmation !== erase)))
            }
            busy={action.busy}
            onClick={submit}
          >
            {mode === 'thresholds'
              ? 'Save thresholds'
              : mode === 'limits'
                ? 'Save limits'
                : mode === 'adopt'
                  ? 'Adopt without erasing'
                  : 'Erase selected disks and continue'}
          </Button>
        }
      >
        {mode === 'thresholds' ? (
          <>
            <Field
              label="Warning used (%)"
              type="number"
              min={1}
              max={98}
              value={warning}
              onChange={(e) => setWarning(Number(e.target.value))}
            />
            <Field
              label="Critical used (%)"
              type="number"
              min={2}
              max={99}
              value={critical}
              onChange={(e) => setCritical(Number(e.target.value))}
            />
            <p>
              Applies to the system disk and every data pool. Warning must be lower than critical.
            </p>
          </>
        ) : mode === 'limits' ? (
          <>
            <Field
              label="Maximum usage (%)"
              type="number"
              min={10}
              max={90}
              value={percent}
              onChange={(e) => setPercent(Number(e.target.value))}
            />
            <Field
              label="Reserved free space (GB)"
              type="number"
              min={20}
              value={reserve}
              onChange={(e) => setReserve(Number(e.target.value))}
            />
            <label>
              <input
                type="checkbox"
                checked={interruption}
                onChange={(e) => setInterruption(e.target.checked)}
              />{' '}
              I acknowledge that shrinking interrupts access to System Pool folders.
            </label>
            <p>
              Limits cannot shrink below existing data plus the safety margin. No files are deleted
              to free space.
            </p>
          </>
        ) : (
          <>
            {mode === 'create' || mode === 'adopt' ? (
              <Field
                label="Pool name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                required
                pattern="[a-z0-9-]{1,32}"
              />
            ) : null}
            <p role="note">
              {mode === 'adopt'
                ? 'The selected ext4 filesystem is mounted without erasing its data.'
                : 'All selected disks will be erased. Loss of any disk loses the whole pool. Keep a backup elsewhere.'}
            </p>
            {eligible.length ? (
              eligible.map((d) => (
                <label className="disk-choice" key={d.serial}>
                  <input
                    type="checkbox"
                    checked={selected.includes(d.serial)}
                    onChange={(e) =>
                      setSelected(
                        e.target.checked
                          ? mode === 'extend' || mode === 'adopt'
                            ? [d.serial]
                            : [...selected, d.serial]
                          : selected.filter((s) => s !== d.serial),
                      )
                    }
                  />
                  {d.model} · {d.serial} · {gb(d.sizeBytes)}
                </label>
              ))
            ) : (
              <p>No eligible data disks are available.</p>
            )}
            {mode !== 'adopt' ? (
              <Field
                label={`Type ${erase}`}
                value={confirmation}
                onChange={(e) => setConfirmation(e.target.value)}
                autoComplete="off"
              />
            ) : null}
          </>
        )}
        {action.error ? <p role="alert">{action.error}</p> : null}
      </Dialog>
    </div>
  );
}
