import type { SystemStatus } from '../../api/view-models.ts';
const expected = [
  ['core', 'FileDock Core'],
  ['management', 'Management Service'],
  ['docker', 'Docker'],
  ['samba', 'Samba'],
  ['nfs', 'NFS'],
  ['kopia', 'Kopia'],
  ['fd-s3', 'S3 Connector'],
  ['rclone', 'rclone'],
];
export function dashboardModel(status: SystemStatus | undefined, browserConnected: boolean) {
  const received = status?.components ?? [];
  const core = received.find((c) => c.id === 'core');
  const sourceAvailable = browserConnected && core?.state === 'Healthy';
  const components = [
    ...expected.map(
      ([id, name]) =>
        received.find((c) => c.id === id) ?? {
          id: id!,
          name: name!,
          state: 'Unknown',
          detail: 'No status reported.',
        },
    ),
    ...received.filter((c) => !expected.some(([id]) => c.id === id)),
  ].map((c) => ({
    ...c,
    freshness: !browserConnected
      ? ('cached' as const)
      : !sourceAvailable && !['core', 'management'].includes(c.id)
        ? ('cached' as const)
        : ('current' as const),
  }));
  const notices = [...(status?.notices ?? [])];
  if (
    status?.notificationChannelMissing &&
    !notices.some((n) => n.toLowerCase().includes('notification channel'))
  )
    notices.push('No notification channel is configured yet.');
  if (status?.stateProtectionWarning)
    notices.push(
      'Appliance state protection needs attention. Recovery coverage has not been verified.',
    );

  return {
    components,
    notices: [...new Set(notices)],
    healthyCount: sourceAvailable ? received.filter((c) => c.state === 'Healthy').length : null,
    reportedCount: received.length,
    sourceAvailable,
  };
}
