import { useQuery } from '@tanstack/react-query';
import { api } from '../../api/client.ts';

type Status = {
  applianceName: string;
  components: { id: string; name: string; state: string; detail: string }[];
  notices: string[];
};

export function DashboardPage({ onSignOut }: { onSignOut: () => void }) {
  const query = useQuery({
    queryKey: ['system-status'],
    queryFn: () => api.get<Status>('/system/status'),
  });

  return (
    <div className="stack">
      <div className="app-header">
        <div>
          <p className="eyebrow">Operations</p>
          <h1>FileDock</h1>
          <p>{query.data?.applianceName || 'Appliance dashboard'}</p>
        </div>
        <button
          className="ghost"
          type="button"
          onClick={async () => {
            await api.post('/auth/sign-out');
            onSignOut();
          }}
        >
          Sign out
        </button>
      </div>
      {query.data?.notices?.length ? <p className="warn">{query.data.notices[0]}</p> : null}
      <div className="cards">
        {(query.data?.components ?? []).map((component) => (
          <article className="card" key={component.id}>
            <h2>
              <span className={`lamp ${component.state.toLowerCase()}`} aria-hidden="true" />
              {component.name}
            </h2>
            <p>{component.state}</p>
            <p className="eyebrow">{component.detail}</p>
          </article>
        ))}
      </div>
    </div>
  );
}
