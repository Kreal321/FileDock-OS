// Synthetic request fixtures only. Never import from production modules.
export const components = [
  ['core', 'FileDock Core'],
  ['management', 'Management Service'],
  ['docker', 'Docker'],
  ['samba', 'Samba'],
  ['nfs', 'NFS'],
  ['kopia', 'Kopia'],
  ['fd-s3', 'S3 Connector'],
  ['rclone', 'rclone'],
].map(([id, name]) => ({ id: id!, name: name!, state: 'Healthy', detail: 'Serving' }));
export const status = {
  applianceName: 'Test appliance',
  interfaceVersion: '1.0',
  releaseVersion: 'test',
  components,
  notices: ['No notification channel is configured yet.'],
  stateProtectionWarning: false,
  notificationChannelMissing: true,
};
export const identity = { id: 'test-admin', username: 'root' };
export const health = { state: 'Healthy', detail: 'Connected', canMutate: true };
export const operation = {
  id: 'op-one',
  kind: 'ReconcileAll',
  state: 'Succeeded',
  steps: [{ name: 'Check units', state: 'Succeeded', attempts: 1, detail: 'Complete' }],
  startedBy: 'root',
  startedAt: '2026-10-13T12:00:00Z',
  finishedAt: '2026-10-13T12:00:01Z',
  error: null,
};
export const activity = {
  id: 'audit-one',
  at: '2026-10-13T12:00:00Z',
  actorName: 'root',
  action: 'admin.created',
  target: 'test-admin',
  result: 'Succeeded',
};
export const states = {
  partial: { ...status, components: components.slice(0, 2) },
  failed: {
    ...status,
    components: components.map((c) =>
      c.id === 'samba' ? { ...c, state: 'Failed', detail: 'Unit failed' } : c,
    ),
  },
  unknown: {
    ...status,
    components: [{ id: 'core', name: 'FileDock Core', state: 'NovelState', detail: '' }],
  },
  empty: { ...status, components: [] },
  stopped: { ...status, components: components.map((c) => ({ ...c, state: 'Stopped' })) },
  degraded: { ...status, components: components.map((c) => ({ ...c, state: 'Degraded' })) },
  cached: {
    ...status,
    components: components.map((c) => (c.id === 'core' ? { ...c, state: 'Unreachable' } : c)),
  },
};
export const problems = {
  setup: 423,
  signin: 401,
  badGateway: 502,
  unavailable: 503,
  expired: 401,
};
export const network = {
  port: 8021,
  listenAddresses: ['::'],
  availableAddresses: [{ interfaceName: 'eth0', address: '192.0.2.1' }],
};
