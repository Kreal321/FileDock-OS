import { dashboardModel } from './dashboard-model.ts';
import { status, states } from '../../../tests/fixtures/ui-states.ts';
test('all eight expected services remain visible with missing data Unknown', () => {
  const view = dashboardModel(states.partial, true);
  expect(view.components).toHaveLength(8);
  expect(view.components.find((c) => c.id === 'docker')?.state).toBe('Unknown');
  expect(view.healthyCount).toBe(2);
});
test('false warning is not proof of recovery coverage and all notices survive', () => {
  const view = dashboardModel({ ...status, notices: ['First notice', 'Second notice'] }, true);
  expect(view.notices).toEqual(
    expect.arrayContaining([
      'First notice',
      'Second notice',
      'Protection information unavailable. Detailed recovery coverage is not available yet.',
    ]),
  );
});
test('cached subsystem values remain visibly stale when Core is unreachable', () => {
  expect(
    dashboardModel(states.cached, true).components.find((c) => c.id === 'samba')?.freshness,
  ).toBe('cached');
  expect(dashboardModel(status, false).healthyCount).toBeNull();
});
