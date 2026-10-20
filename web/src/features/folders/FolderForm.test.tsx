import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { FolderForm, type Options } from './FolderForm.tsx';
afterEach(cleanup);
const options: Options = {
  modes: [
    { id: 'LocalOnly', enabled: true, missingCapabilities: [] },
    { id: 'HybridBackup', enabled: true, missingCapabilities: [] },
    { id: 'CloudMount', enabled: false, missingCapabilities: ['cache.media'] },
    { id: 'TieredStorage', enabled: false, missingCapabilities: ['tier.storage'] },
  ],
  pools: [],
  connections: [],
  access: [],
};
it('asks for importance only in Hybrid Backup with no default', () => {
  render(<FolderForm options={options} canMutate onSaved={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.queryByRole('combobox', { name: 'Data importance' })).not.toBeInTheDocument();
  fireEvent.change(screen.getByRole('combobox', { name: 'Storage mode' }), {
    target: { value: 'HybridBackup' },
  });
  expect(screen.getByRole('combobox', { name: 'Data importance' })).toHaveValue('');
  expect(screen.getByRole('button', { name: 'Save folder' })).toBeDisabled();
  fireEvent.change(screen.getByRole('combobox', { name: 'Storage mode' }), {
    target: { value: 'LocalOnly' },
  });
  expect(screen.queryByRole('combobox', { name: 'Data importance' })).not.toBeInTheDocument();
});
it('names missing capabilities and disables changes when mutation is unavailable', () => {
  render(<FolderForm options={options} canMutate={false} onSaved={vi.fn()} onCancel={vi.fn()} />);
  expect(screen.getByRole('option', { name: /Cloud Mount.*cache.media/ })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Save folder' })).toBeDisabled();
  expect(
    screen.getByText(/Data kept only on this appliance has no off-appliance copy/),
  ).toBeInTheDocument();
});

it('shows the actual provider mode observation as the durable folder-data point', () => {
  render(
    <FolderForm
      options={options}
      canMutate
      onSaved={vi.fn()}
      onCancel={vi.fn()}
      folder={{
        id: 'fixture',
        generation: 1,
        identity: { name: 'important', fileDockPath: '/storage/important' },
        storageMode: 'HybridBackup',
        reference: { poolId: 'fixture', relativePath: '/important' },
        dataImportance: 'Important',
        access: [],
        advanced: {},
        protectionState: 'Protected',
        exposure: '',
        status: {
          state: 'Ready',
          detail: '',
          modeObservation: { lastDurableRecoveryPoint: '2026-10-19T12:00:00Z' },
        },
      }}
    />,
  );
  expect(
    screen.getByText(/Latest durable folder-data recovery point/).querySelector('time'),
  ).toHaveAttribute('datetime', '2026-10-19T12:00:00Z');
});
it('shows all six sections expanded even when importance is not applicable', () => {
  const { container } = render(
    <FolderForm options={options} canMutate onSaved={vi.fn()} onCancel={vi.fn()} />,
  );
  expect(screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent)).toEqual([
    '1. Identity',
    '2. Storage Mode',
    '3. Source / Backend',
    '4. Data Importance',
    '5. Access',
    '6. Advanced',
  ]);
  expect(container.querySelector('details')).toBeNull();
  expect(screen.getByText(/only applies to Hybrid Backup/)).toBeVisible();
});
