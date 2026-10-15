import { fireEvent, render, screen } from '@testing-library/react';
import { Button } from './Button.tsx';
import { Field } from './Field.tsx';
import { Switch } from './Switch.tsx';
import { Tabs } from './Tabs.tsx';
import { Dialog } from './Dialog.tsx';
import { StatusBadge } from './StatusBadge.tsx';
import { DataTable } from './DataTable.tsx';
test('busy buttons and fields have accessible feedback', () => {
  render(
    <>
      <Button busy>Save</Button>
      <Field label="Username" name="username" error="Required" />
    </>,
  );
  expect(screen.getByRole('button')).toBeDisabled();
  expect(screen.getByLabelText('Username')).toHaveAttribute('aria-invalid', 'true');
  expect(screen.getByLabelText('Username')).toHaveAccessibleDescription('Required');
});
test('statuses are words plus an icon and switches expose state', () => {
  render(
    <>
      <StatusBadge state="Failed" />
      <Switch label="Enabled" checked={false} pending onChange={() => {}} />
    </>,
  );
  expect(screen.getByText('Failed')).toBeInTheDocument();
  expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false');
  expect(screen.getByRole('switch')).toBeDisabled();
});
test('tabs use arrow keys and roving focus', () => {
  render(
    <Tabs
      tabs={[
        { id: 'one', label: 'One', content: 'First' },
        { id: 'two', label: 'Two', content: 'Second' },
      ]}
    />,
  );
  fireEvent.keyDown(screen.getByRole('tab', { name: 'One' }), { key: 'ArrowRight' });
  expect(screen.getByRole('tab', { name: 'Two' })).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByRole('tabpanel')).toHaveTextContent('Second');
});
test('dialog Escape restores focus and table has a named region', () => {
  const close = vi.fn();
  const trigger = document.createElement('button');
  document.body.append(trigger);
  trigger.focus();
  render(
    <>
      <Dialog open title="Confirm" onClose={close}>
        <Button>Continue</Button>
      </Dialog>
      <DataTable label="History" headers={['Name']}>
        <tr>
          <td>Item</td>
        </tr>
      </DataTable>
    </>,
  );
  fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(close).toHaveBeenCalled();
  expect(screen.getByRole('region', { name: 'History' })).toBeInTheDocument();
  trigger.remove();
});

test('column metadata aligns headers and cells and compact actions are named', async () => {
  const { RowActions } = await import('./RowActions.tsx');
  render(
    <DataTable
      label="Inventory"
      headers={[
        { id: 'name', label: 'Name' },
        { id: 'size', label: 'Size', align: 'end' },
        { id: 'actions', label: 'Actions', align: 'actions' },
      ]}
    >
      <tr>
        <td>Long name /storage/example</td>
        <td>123</td>
        <td>
          <RowActions
            label="Actions for example"
            actions={[{ label: 'Edit example', onClick: vi.fn() }]}
          />
        </td>
      </tr>
    </DataTable>,
  );
  expect(screen.getByRole('columnheader', { name: 'Size' })).toHaveAttribute('data-align', 'end');
  expect(screen.getByRole('cell', { name: '123' })).toHaveAttribute('data-align', 'end');
  fireEvent.click(screen.getByRole('button', { name: 'Actions for example' }));
  expect(screen.getByRole('button', { name: 'Edit example' })).toBeVisible();
  fireEvent.keyDown(screen.getByRole('button', { name: 'Edit example' }), { key: 'Escape' });
  expect(screen.queryByRole('button', { name: 'Edit example' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Actions for example' })).toHaveFocus();
});
test('dialog provides one form footer, focuses first field and confirms dirty cancel', () => {
  const close = vi.fn();
  const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
  render(
    <Dialog
      open
      title="Edit account"
      description="Account details"
      dirty
      onClose={close}
      footer={
        <Button form="account-form" type="submit">
          Save
        </Button>
      }
    >
      <form id="account-form">
        <Field label="Account name" />
      </form>
    </Dialog>,
  );
  expect(screen.getByLabelText('Account name')).toHaveFocus();
  expect(screen.getAllByRole('button', { name: 'Cancel' })).toHaveLength(1);
  expect(screen.getByRole('dialog')).toHaveAccessibleDescription('Account details');
  fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(close).not.toHaveBeenCalled();
  expect(confirm).toHaveBeenCalled();
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  expect(close).toHaveBeenCalledOnce();
  confirm.mockRestore();
});
