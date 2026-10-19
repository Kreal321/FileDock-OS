import { RecoveryPoint } from '../../components/RecoveryPoint.tsx';
import { RestoreReport } from '../restore/RestoreReport.tsx';
import type { Protection } from '../recovery/StateProtectionPage.tsx';
import { useQuery } from '@tanstack/react-query';
import { api } from '../../api/client.ts';
import {
  dataView,
  displayTime,
  type SystemStatus,
  type OperationPage,
  type ActivityPage,
} from '../../api/view-models.ts';
import { PageHeader } from '../../components/PageHeader.tsx';
import { Panel } from '../../components/Panel.tsx';
import { Notice } from '../../components/Notice.tsx';
import { Icon } from '../../components/Icon.tsx';
import { StateView } from '../../components/StateView.tsx';
import { StatusBadge } from '../../components/StatusBadge.tsx';
import { dashboardModel } from './dashboard-model.ts';
export function DashboardPage({ connected = true }: { connected?: boolean }) {
  const protection = useQuery({
    queryKey: ['state-protection'],
    queryFn: () => api.get<Protection>('/state-protection'),
    refetchInterval: 10000,
  });
  const q = useQuery({
    queryKey: ['system-status'],
    queryFn: () => api.get<SystemStatus>('/system/status'),
    retry: false,
  });
  const ops = useQuery({
    queryKey: ['operations', 'preview'],
    queryFn: () => api.get<OperationPage>('/operations?pageSize=5'),
    retry: false,
  });
  const audit = useQuery({
    queryKey: ['audit', 'preview'],
    queryFn: () => api.get<ActivityPage>('/audit?pageSize=5'),
    retry: false,
  });
  const view = dashboardModel(q.data, connected && !q.error);
  const stale = !view.sourceAvailable;
  return (
    <div className="stack">
      <PageHeader
        title="Dashboard"
        description={`${q.data?.applianceName || 'Appliance overview'} · Service health and administrative activity`}
      />
      <StateView
        {...dataView(q.data, q.isPending, q.error, q.dataUpdatedAt, stale)}
        onRetry={() => void q.refetch()}
      />
      <div className="summary-grid">
        <Panel className="summary-card">
          <span className="icon-tile">
            <Icon name="service" />
          </span>
          <div>
            <p className="caption muted">Reported services</p>
            <p className="metric">{q.data ? view.reportedCount : '—'}</p>
            <p className="caption muted">Records received</p>
          </div>
        </Panel>
        <Panel className="summary-card">
          <span className={`icon-tile ${stale ? 'warning' : 'success'}`}>
            <Icon name={stale ? 'warning' : 'success'} />
          </span>
          <div>
            <p className="caption muted">Healthy services</p>
            <p className="metric">{view.healthyCount ?? '—'}</p>
            <p className="caption muted">
              {stale ? 'Current status unavailable' : 'From reported records'}
            </p>
          </div>
        </Panel>
        <Panel className="summary-card">
          <span className="icon-tile">
            <Icon name="info" />
          </span>
          <div>
            <p className="caption muted">Release</p>
            <p className="metric">{q.data?.releaseVersion || '—'}</p>
            <p className="caption muted">
              {q.data?.interfaceVersion
                ? `Interface ${q.data.interfaceVersion}`
                : 'Version not reported'}
            </p>
          </div>
        </Panel>
        <Panel className="summary-card">
          <span className="icon-tile warning">
            <Icon name="warning" />
          </span>
          <div>
            <p className="caption muted">Recovery coverage</p>
            <p className="metric">
              {protection.error
                ? 'Unknown'
                : protection.data
                  ? protection.data.machineLossCovered
                    ? 'Machine loss covered'
                    : 'At risk'
                  : 'Unknown'}
            </p>
            <p className="caption muted">
              {protection.error
                ? 'Cached status; current coverage unavailable'
                : protection.data
                  ? `Reinstall ${protection.data.reinstallCovered ? 'covered' : 'not covered'}`
                  : 'Protection status unavailable'}
            </p>
          </div>
        </Panel>
      </div>
      {view.notices.map((n) => (
        <Notice key={n} tone="warning">
          {n}
        </Notice>
      ))}
      <RecoveryPoint
        label="Latest durable configuration recovery point"
        at={protection.data?.copies
          ?.map((copy) => copy.lastSnapshotAt)
          .filter((at): at is string => Boolean(at && Number.isFinite(Date.parse(at))))
          .sort()
          .at(-1)}
        cached={!connected || Boolean(protection.error)}
      />
      <a href="#/settings/state-protection">Review recovery copies and loss exposure</a>
      <RestoreReport />
      <div className="dashboard-grid">
        <Panel className="stack">
          <div className="panel-heading">
            <h2>Service health</h2>
            <span className="caption muted">Core and built-in subsystems</span>
          </div>
          <div className="service-grid">
            {view.components.map((c) => (
              <article className="service-card" key={c.id}>
                <div className="service-title">
                  <Icon name="service" />
                  <h3>{c.name}</h3>
                </div>
                <StatusBadge state={c.state} freshness={q.error ? 'cached' : c.freshness} />
                <p className="caption muted">{c.detail || 'No detail reported.'}</p>
              </article>
            ))}
          </div>
          {q.dataUpdatedAt ? (
            <p className="caption muted">
              Last received: {new Date(q.dataUpdatedAt).toLocaleString()} · Local receipt time
            </p>
          ) : null}
        </Panel>
        <div className="stack">
          <Panel className="stack">
            <div className="panel-heading">
              <h2>Recent operations</h2>
              <a href="#/settings/operations">View all</a>
            </div>
            <StateView
              {...dataView(
                ops.data?.operations,
                ops.isPending,
                ops.error,
                ops.dataUpdatedAt,
                stale,
              )}
              onRetry={() => void ops.refetch()}
              empty="No operations yet."
            />
            <div className="record-list">
              {ops.data?.operations.slice(0, 5).map((op) => (
                <div key={op.id} className="record">
                  <div>
                    <a href={`#/settings/operations/${encodeURIComponent(op.id)}`}>{op.kind}</a>
                    <p className="caption muted">{displayTime(op.startedAt)}</p>
                  </div>
                  <StatusBadge
                    state={op.state}
                    freshness={stale || ops.error ? 'cached' : 'current'}
                  />
                </div>
              ))}
            </div>
          </Panel>
          <Panel className="stack">
            <div className="panel-heading">
              <h2>Administrative activity</h2>
              <a href="#/settings/audit">View all</a>
            </div>
            <StateView
              {...dataView(
                audit.data?.items,
                audit.isPending,
                audit.error,
                audit.dataUpdatedAt,
                !connected,
              )}
              onRetry={() => void audit.refetch()}
              empty="No administrative activity yet."
            />
            <div className="record-list">
              {audit.data?.items.slice(0, 5).map((item) => (
                <div key={item.id} className="record">
                  <div>
                    <p>{item.action}</p>
                    <p className="caption muted">
                      {item.actorName} · {item.target}
                    </p>
                    <p className="caption muted">{displayTime(item.at)}</p>
                  </div>
                  <StatusBadge
                    state={item.result}
                    freshness={!connected || audit.error ? 'cached' : 'current'}
                  />
                </div>
              ))}
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}
