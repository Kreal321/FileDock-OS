import { QueryClient, QueryClientProvider, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useState } from 'react';
import { ApiError, createApiClient } from './api/client.ts';
import { SignInPage } from './features/auth/SignInPage.tsx';
import { SetupWizard } from './features/setup/SetupWizard.tsx';
import { DashboardPage } from './features/dashboard/DashboardPage.tsx';
import { AdminsPage } from './features/admins/AdminsPage.tsx';
import { TokensPage } from './features/tokens/TokensPage.tsx';
import { NetworkPage } from './features/settings/NetworkPage.tsx';
import { SettingsPage } from './features/settings/SettingsPage.tsx';
import { OperationsPage } from './features/operations/OperationsPage.tsx';
import { OperationDetailPage } from './features/operations/OperationDetailPage.tsx';
import { AuditPage } from './features/audit/AuditPage.tsx';
import { AuthLayout } from './layout/AuthLayout.tsx';
import { AppShell } from './layout/AppShell.tsx';
import { StateProtectionPage } from './features/recovery/StateProtectionPage.tsx';
import { LocationsPage } from './features/recovery/LocationsPage.tsx';
import { ExtensionsPage } from './features/extensions/ExtensionsPage.tsx';
import { Panel } from './components/Panel.tsx';
import { Button } from './components/Button.tsx';
import { useHashRoute, resetNavigation } from './navigation/useHashRoute.ts';
const gateApi = createApiClient({ notifySessionExpiry: false });
async function readGate() {
  try {
    await gateApi.get('/system/status');
    return 'ready' as const;
  } catch (e) {
    if (e instanceof ApiError && e.status === 423) return 'setup' as const;
    if (e instanceof ApiError && e.status === 401) return 'signin' as const;
    throw e;
  }
}
function Shell() {
  const cache = useQueryClient();
  const route = useHashRoute();
  const [signedOut, setSignedOut] = useState(false);
  const signOut = useCallback(() => {
    setSignedOut(true);
    resetNavigation();
    void cache.cancelQueries({ predicate: (q) => q.queryKey[0] !== 'session-gate' });
    cache.removeQueries({ predicate: (q) => q.queryKey[0] !== 'session-gate' });
    cache.setQueryData(['session-gate'], 'signin');
  }, [cache]);
  useEffect(() => {
    window.addEventListener('filedock:session-expired', signOut);
    return () => window.removeEventListener('filedock:session-expired', signOut);
  }, [signOut]);
  const gate = useQuery({
    queryKey: ['session-gate'],
    queryFn: readGate,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const refresh = () => {
    setSignedOut(false);
    void gate.refetch();
  };
  const view = signedOut ? 'signin' : gate.data;
  if (gate.isPending && !signedOut)
    return (
      <AuthLayout>
        <Panel className="stack">
          <h1>Checking the appliance…</h1>
          <p role="status">Connecting to FileDock.</p>
        </Panel>
      </AuthLayout>
    );
  if (gate.isError && !signedOut)
    return (
      <AuthLayout>
        <Panel className="stack">
          <h1>Appliance unavailable</h1>
          <p role="alert">Cannot reach the appliance. Check its connection and try again.</p>
          <Button onClick={refresh} busy={gate.isFetching}>
            Retry
          </Button>
        </Panel>
      </AuthLayout>
    );
  if (view === 'setup')
    return (
      <AuthLayout>
        <SetupWizard onDone={refresh} />
      </AuthLayout>
    );
  if (view !== 'ready')
    return (
      <AuthLayout>
        <SignInPage onSignedIn={refresh} />
      </AuthLayout>
    );
  return (
    <AppShell route={route} onSignOut={signOut}>
      {(canMutate, connected, browserConnected) => {
        switch (route.id) {
          case 'extensions':
            return <ExtensionsPage canMutate={canMutate && browserConnected} />;
          case 'state-protection':
            return <StateProtectionPage canMutate={canMutate && browserConnected} />;
          case 'locations':
            return <LocationsPage canMutate={canMutate && browserConnected} />;
          case 'settings':
            return <SettingsPage />;
          case 'administrators':
            return (
              <AdminsPage
                connected={browserConnected}
                canMutate={canMutate}
                onSignedOut={signOut}
              />
            );
          case 'tokens':
            return <TokensPage connected={browserConnected} canMutate={canMutate} />;
          case 'network':
            return <NetworkPage connected={browserConnected} canMutate={canMutate} />;
          case 'operations':
            return <OperationsPage connected={connected} />;
          case 'operation-detail':
            return <OperationDetailPage id={route.operationId!} connected={connected} />;
          case 'audit':
            return <AuditPage connected={browserConnected} />;
          default:
            return <DashboardPage connected={connected} />;
        }
      }}
    </AppShell>
  );
}
export function App() {
  const [cache] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  return (
    <QueryClientProvider client={cache}>
      <Shell />
    </QueryClientProvider>
  );
}
