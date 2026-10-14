export type Tone = 'success' | 'warning' | 'danger' | 'info' | 'neutral';
export type AuthenticatedIdentity = { id: string; username: string; displayInitials: string };
export type NavigationDestination = {
  id: string;
  hashRoute: string;
  label: string;
  icon: string;
  group: 'main' | 'settings';
  availability: 'implemented';
  requiredView: string;
};
export type DataView<T> = {
  phase: 'loading' | 'ready' | 'empty' | 'error' | 'stale' | 'unavailable';
  data?: T;
  lastSuccessfulFetchAt?: number;
  error?: string;
  retryable: boolean;
};
export type FormViewState = 'pristine' | 'dirty' | 'submitting' | 'saved' | 'error';
export type ComponentHealth = { id: string; name: string; state: string; detail: string };
export type SystemStatus = {
  applianceName: string;
  releaseVersion?: string;
  interfaceVersion?: string;
  components: ComponentHealth[];
  notices: string[];
  stateProtectionWarning?: boolean;
  notificationChannelMissing?: boolean;
};
export type OperationView = {
  id: string;
  kind: string;
  state: string;
  startedBy: string;
  startedAt: string | null;
  finishedAt: string | null;
  error: string | null;
  steps: { name: string; state: string; attempts: number; detail: string }[];
};
export type OperationPage = { operations: OperationView[]; nextPageToken: string | null };
export type AdministrativeActivityView = {
  id: string;
  at: string;
  actorName: string;
  action: string;
  target: string;
  result: string;
};
export type ActivityPage = { items: AdministrativeActivityView[]; nextPageToken: string | null };
export function identityView(identity: { id: string; username: string }): AuthenticatedIdentity {
  return { ...identity, displayInitials: identity.username.slice(0, 2).toUpperCase() };
}
export function effectiveCanMutate(browserStreamConnected: boolean, core?: { canMutate: boolean }) {
  return browserStreamConnected && core?.canMutate === true;
}
export function statusPresentation(
  rawState: string,
  freshness: 'current' | 'cached' | 'unavailable' = 'current',
) {
  const tone: Tone = ['Healthy', 'Succeeded', 'Enabled', 'Active', 'Protected'].includes(rawState)
    ? 'success'
    : ['Failed', 'Unreachable', 'Incompatible', 'Error', 'Critical'].includes(rawState)
      ? 'danger'
      : ['Degraded', 'Starting', 'Pending', 'Running', 'Warning', 'AtRisk'].includes(rawState)
        ? 'warning'
        : 'neutral';
  return {
    rawState,
    label:
      rawState === 'AtRisk'
        ? 'At Risk'
        : rawState === 'NotRequired'
          ? 'Not Required'
          : rawState || 'Unknown',
    tone: freshness === 'current' ? tone : ('neutral' as Tone),
    freshness,
  };
}
export function dataView<T>(
  data: T | undefined,
  pending: boolean,
  error: Error | null,
  lastSuccessfulFetchAt?: number,
  stale = false,
): DataView<T> {
  const phase =
    data !== undefined
      ? error || stale
        ? 'stale'
        : Array.isArray(data) && data.length === 0
          ? 'empty'
          : 'ready'
      : pending
        ? 'loading'
        : error
          ? 'unavailable'
          : 'unavailable';
  return { phase, data, lastSuccessfulFetchAt, error: error?.message, retryable: !!error };
}
export function displayTime(value: string | null | undefined) {
  if (!value) return 'Not reported';
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? 'Not reported' : d.toLocaleString();
}
