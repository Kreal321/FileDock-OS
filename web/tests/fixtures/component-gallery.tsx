/* eslint-disable react-refresh/only-export-components -- Test-only dynamic bootstrap exports mount, never ships with the app. */
// Browser-only component review harness; excluded from the production entry/build.
import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { Button } from '../../src/components/Button.tsx';
import { Field } from '../../src/components/Field.tsx';
import { Select } from '../../src/components/Select.tsx';
import { Switch } from '../../src/components/Switch.tsx';
import { Tabs } from '../../src/components/Tabs.tsx';
import { Dialog } from '../../src/components/Dialog.tsx';
import { StatusBadge } from '../../src/components/StatusBadge.tsx';
import { Notice } from '../../src/components/Notice.tsx';
import { StateView } from '../../src/components/StateView.tsx';
import { DataTable } from '../../src/components/DataTable.tsx';
function Gallery() {
  const [tab, setTab] = useState('one');
  const [open, setOpen] = useState(false);
  return (
    <main className="main-content stack">
      <h1>Component review</h1>
      <section className="panel stack">
        <h2>Actions</h2>
        <div className="actions">
          <Button>Primary</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="danger">Danger</Button>
          <Button busy>Saving</Button>
          <Button disabled>Disabled</Button>
        </div>
        <Field label="Required name" error="Please enter a name." />
        <Select label="Selection" error="Selection could not be saved.">
          <option>Option one</option>
        </Select>
        <Switch label="Enabled" checked onChange={() => {}} />
        <Switch label="Saving switch" checked={false} pending onChange={() => {}} />
        <Switch label="Failed switch" checked={false} error="Could not save." onChange={() => {}} />
      </section>
      <section className="panel stack">
        <h2>Status and feedback</h2>
        <div className="actions">
          {['Healthy', 'Degraded', 'Failed', 'Stopped', 'Unknown'].map((state) => (
            <StatusBadge key={state} state={state} />
          ))}
        </div>
        <StatusBadge state="Healthy" freshness="cached" />
        <Notice tone="info">Information</Notice>
        <Notice tone="success">This change was saved.</Notice>
        <Notice>Recovery needs attention.</Notice>
        <Notice tone="danger">This operation failed.</Notice>
        <StateView phase="loading" />
        <StateView phase="empty" />
        <StateView phase="unavailable" error="Connection unavailable" onRetry={() => {}} />
        <StateView phase="stale" error="Connection lost" onRetry={() => {}} />
      </section>
      <section className="panel stack">
        <h2>Navigation and data</h2>
        <Tabs
          selected={tab}
          onSelect={setTab}
          tabs={[
            { id: 'one', label: 'Overview', content: 'Overview content' },
            { id: 'two', label: 'Details', content: 'Details content' },
          ]}
        />
        <DataTable label="Long values" headers={['Name', 'Status', 'Action']}>
          <tr>
            <td>{'A long resource name '.repeat(12)}</td>
            <td>
              <StatusBadge state="Failed" />
            </td>
            <td>
              <Button variant="secondary">Inspect</Button>
            </td>
          </tr>
        </DataTable>
        <Button variant="danger" onClick={() => setOpen(true)}>
          Open confirmation
        </Button>
        <Dialog open={open} title="Confirm action" onClose={() => setOpen(false)}>
          <p>Review the affected resource before continuing.</p>
          <Button variant="danger">Confirm</Button>
        </Dialog>
      </section>
    </main>
  );
}
export function mount() {
  const root = document.getElementById('root');
  if (root) createRoot(root).render(<Gallery />);
}
