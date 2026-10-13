import { QueryClient, QueryClientProvider, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useState } from 'react';
import { ApiError, createApiClient } from './api/client.ts';
import { SignInPage } from './features/auth/SignInPage.tsx';
import { DashboardPage } from './features/dashboard/DashboardPage.tsx';
import { AdminsPage } from './features/admins/AdminsPage.tsx';
import { TokensPage } from './features/tokens/TokensPage.tsx';
import { NetworkPage } from './features/settings/NetworkPage.tsx';
import { CoreStatusBanner } from './components/CoreStatusBanner.tsx';
import { useLiveCoreStatus } from './api/useLiveCoreStatus.ts';
import { SetupWizard } from './features/setup/SetupWizard.tsx';

// A signed-out startup probe must resolve the gate without clearing its own query.
const gateApi = createApiClient({ notifySessionExpiry: false });

type Gate = 'setup' | 'signin' | 'ready';

async function readGate(): Promise<Gate> {
  try {
    await gateApi.get('/system/status');
    return 'ready';
  } catch (error) {
    if (error instanceof ApiError && error.status === 423) {
      return 'setup';
    }
    return 'signin';
  }
}

function Shell() {
  const queryClient = useQueryClient();
  const [signedOut, setSignedOut] = useState(false);
  const [page, setPage] = useState<'dashboard' | 'admins' | 'tokens' | 'network'>('dashboard');
  const signOut = useCallback(() => {
    queryClient.removeQueries({ predicate: (query) => query.queryKey[0] !== 'session-gate' });
    setPage('dashboard');
    setSignedOut(true);
  }, [queryClient]);
  useEffect(() => {
    window.addEventListener('filedock:session-expired', signOut);
    return () => window.removeEventListener('filedock:session-expired', signOut);
  }, [signOut]);
  const gate = useQuery({ queryKey: ['session-gate'], queryFn: readGate });
  const view = signedOut ? 'signin' : (gate.data ?? 'signin');

  return (
    <div className="app-shell">
      {gate.isPending && !signedOut ? <p>Checking the appliance…</p> : null}
      {view === 'setup' ? (
        <SetupWizard
          onDone={() => {
            setSignedOut(false);
            void gate.refetch();
          }}
        />
      ) : null}
      {view === 'signin' && (!gate.isPending || signedOut) ? (
        <SignInPage
          onSignedIn={() => {
            setSignedOut(false);
            void gate.refetch();
          }}
        />
      ) : null}
      {view === 'ready' ? <Appliance page={page} setPage={setPage} signOut={signOut} /> : null}
    </div>
  );
}

function Appliance({
  page,
  setPage,
  signOut,
}: {
  page: 'dashboard' | 'admins' | 'tokens' | 'network';
  setPage: (page: 'dashboard' | 'admins' | 'tokens' | 'network') => void;
  signOut: () => void;
}) {
  const health = useLiveCoreStatus();
  return (
    <div className="stack">
      <CoreStatusBanner health={health} />
      <nav className="navigation" aria-label="Main navigation">
        {(['dashboard', 'admins', 'tokens', 'network'] as const).map((item) => (
          <button
            className="ghost"
            key={item}
            aria-current={page === item ? 'page' : undefined}
            onClick={() => setPage(item)}
          >
            {item === 'admins'
              ? 'Administrators'
              : item === 'network'
                ? 'Network settings'
                : item === 'tokens'
                  ? 'API tokens'
                  : 'Dashboard'}
          </button>
        ))}
      </nav>
      {page === 'dashboard' ? <DashboardPage onSignOut={signOut} /> : null}
      <fieldset className="change-controls stack" disabled={!health?.canMutate}>
        {page === 'admins' ? <AdminsPage onSignedOut={signOut} /> : null}
        {page === 'tokens' ? <TokensPage /> : null}
        {page === 'network' ? <NetworkPage /> : null}
      </fieldset>
    </div>
  );
}

export function App() {
  const [queryClient] = useState(() => new QueryClient());
  return (
    <QueryClientProvider client={queryClient}>
      <Shell />
    </QueryClientProvider>
  );
}
