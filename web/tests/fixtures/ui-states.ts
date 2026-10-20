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

// Layout-revision fixture inventory: illustrative/test-only, no production fallbacks.
export const layoutStates = {
  settingsChildren: [
    'administrators',
    'tokens',
    'network',
    'notification-channels',
    'state-protection',
    'locations',
    'operations',
    'activity',
    'alerts',
    'logs',
    'audit',
  ],
  subsystems: [
    {
      subsystemId: 'notify-test',
      name: 'Notification recorder',
      version: '1.0.0',
      class: 'Extension',
      runtimeType: 'native-process',
      state: 'Running',
      capabilities: ['notify.channel'],
      dependentFolders: [],
      dataBindings: {},
      dataProtected: false,
    },
    {
      subsystemId: 'docker',
      name: 'Docker',
      version: '1.0.0',
      class: 'BuiltIn',
      runtimeType: 'systemd-service',
      state: 'Running',
      capabilities: ['container.runtime'],
      dependentFolders: [],
      dataBindings: {},
      dataProtected: false,
    },
    {
      subsystemId: 'core',
      name: 'Core',
      version: '1.0.0',
      class: 'Core',
      runtimeType: 'native-process',
      state: 'Healthy',
      capabilities: [],
      dependentFolders: [],
      dataBindings: {},
      dataProtected: false,
    },
  ],
  folders: [
    {
      id: 'folder-one',
      generation: 2,
      identity: {
        name: 'A long folder name for alignment',
        fileDockPath: '/storage/a-long-folder-name-for-alignment',
      },
      storageMode: 'LocalOnly',
      dataImportance: null,
      access: ['Docker'],
      advanced: {},
      reference: { poolId: 'pool-one', relativePath: '/data' },
      protectionState: 'Unprotected',
      exposure: 'Only copy on this appliance',
    },
  ],
  modes: [
    { id: 'LocalOnly', enabled: true, missingCapabilities: [] },
    { id: 'HybridBackup', enabled: false, missingCapabilities: ['backup.snapshot'] },
    { id: 'CloudMount', enabled: false, missingCapabilities: ['cloud.mount'] },
    { id: 'TieredStorage', enabled: false, missingCapabilities: ['tier.storage'] },
  ],
  channels: {
    items: [
      {
        id: 'channel-one',
        name: 'Test delivery',
        subsystemId: 'notify-test',
        minSeverity: 'Warning',
        config: {},
        secretRefs: [],
      },
    ],
  },
  recovery: {
    remoteDestinationRegistered: false,
    reinstallCovered: true,
    machineLossCovered: false,
    warning: true,
    copies: [
      {
        kind: 'system',
        description: 'System disk',
        lastSnapshotAt: '2026-10-13T12:00:00Z',
        stale: false,
      },
    ],
  },
  pages: {
    empty: { items: [], nextPageToken: null },
    error: { status: 503, detail: 'Unavailable' },
    conflict: { status: 409, detail: 'Generation changed' },
  },
};

export const folderModeFixtures = ['LocalOnly', 'HybridBackup', 'CloudMount', 'TieredStorage'].map(
  (storageMode, i) => ({
    ...layoutStates.folders[0],
    id: `folder-mode-${i}`,
    storageMode,
    dataImportance: storageMode === 'HybridBackup' ? 'Critical' : null,
    protectionState:
      storageMode === 'HybridBackup'
        ? 'AtRisk'
        : storageMode === 'CloudMount'
          ? 'Protected'
          : 'Unprotected',
    reference:
      storageMode === 'CloudMount'
        ? { connectionId: 'connection-one', remotePath: 'media' }
        : storageMode === 'TieredStorage'
          ? {
              poolId: 'pool-one',
              relativePath: `/data-${i}`,
              connectionId: 'connection-one',
              remotePath: 'tier',
            }
          : storageMode === 'HybridBackup'
            ? {
                poolId: 'pool-one',
                relativePath: `/data-${i}`,
                backupDestination: { connectionId: 'connection-one', remotePath: 'backup' },
              }
            : { poolId: 'pool-one', relativePath: `/data-${i}` },
    advanced:
      storageMode === 'HybridBackup'
        ? {
            backupEnabled: true,
            backupIntervalSeconds: 300,
            retention: { keepLast: 30 },
            subfolderImportance: {},
          }
        : storageMode === 'CloudMount'
          ? { cache: { type: 'Generic', capacityGb: 100 }, consistency: 'Balanced' }
          : storageMode === 'TieredStorage'
            ? { coldAfterDays: 30 }
            : {},
  }),
);

export const folderOptionsFixture = {
  modes: layoutStates.modes,
  pools: [{ id: 'pool-one', name: 'Data pool', kind: 'Adopted', status: { state: 'Ready' } }],
  connections: [{ id: 'connection-one', name: 'Synthetic object storage' }],
  access: [
    { id: 'Docker', enabled: true },
    { id: 'Smb', enabled: false, missingCapability: 'protocol.smb' },
  ],
};

// Phase 14 synthetic streams: Logs deliberately have no fabricated persistent ID.
export const addonStates = {
  administrators: [{ ...identity, enabled: true, lockedUntil: null }],
  tokens: [
    {
      id: 'test-token',
      name: 'Automation',
      scope: 'ReadOnly',
      expiresAt: null,
      revokedAt: null,
      lastUsedAt: null,
    },
  ],
  activity: {
    items: [
      {
        seq: 1,
        at: '2026-10-19T12:00:00Z',
        type: 'operation.completed',
        subject: { kind: 'Operation', id: 'op-one' },
        payload: { detail: 'Synthetic job complete' },
      },
    ],
    nextPageToken: 'activity-next',
  },
  logs: {
    items: [
      {
        at: '2026-10-19T12:00:00Z',
        source: 'filedock-core.service',
        severity: 'Warning',
        message: 'Synthetic diagnostic\n' + 'Complete diagnostic detail. '.repeat(80),
      },
    ],
    nextPageToken: 'logs-next',
  },
  audit: { items: [activity], nextPageToken: 'audit-next' },
  channels: layoutStates.channels,
  failure: { status: 503, body: { detail: 'Synthetic service unavailable' } },
  delayedResponseMs: 200,
};
