import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { ApiError, api } from './api/client.ts';
import { SignInPage } from './features/auth/SignInPage.tsx';
import { DashboardPage } from './features/dashboard/DashboardPage.tsx';
import { SetupWizard } from './features/setup/SetupWizard.tsx';

const queryClient = new QueryClient();

type Gate = 'setup' | 'signin' | 'ready';

async function readGate(): Promise<Gate> {
  try {
    await api.get('/system/status');
    return 'ready';
  } catch (error) {
    if (error instanceof ApiError && error.status === 423) {
      return 'setup';
    }
    return 'signin';
  }
}

function Shell() {
  const [signedOut, setSignedOut] = useState(false);
  const gate = useQuery({ queryKey: ['session-gate'], queryFn: readGate });
  const view = signedOut ? 'signin' : (gate.data ?? 'signin');

  return (
    <div className="app-shell">
      {gate.isPending ? <p>Checking the appliance…</p> : null}
      {view === 'setup' ? (
        <SetupWizard
          onDone={() => {
            setSignedOut(false);
            void gate.refetch();
          }}
        />
      ) : null}
      {view === 'signin' && !gate.isPending ? (
        <SignInPage
          onSignedIn={() => {
            setSignedOut(false);
            void gate.refetch();
          }}
        />
      ) : null}
      {view === 'ready' ? <DashboardPage onSignOut={() => setSignedOut(true)} /> : null}
    </div>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <Shell />
    </QueryClientProvider>
  );
}
