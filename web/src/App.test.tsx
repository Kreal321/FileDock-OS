import { render, screen } from '@testing-library/react';
import { App } from './App.tsx';

test('renders the first-run setup gate when the API is locked', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ status: 423, title: 'Setup required' }), {
        status: 423,
        headers: { 'content-type': 'application/problem+json' },
      }),
    ),
  );
  render(<App />);
  expect(await screen.findByText(/one-time setup code/i)).toBeInTheDocument();
});

test('an expected startup 401 opens sign-in without expiring the session gate', async () => {
  const expired = vi.fn();
  window.addEventListener('filedock:session-expired', expired);
  vi.stubGlobal(
    'fetch',
    vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ status: 401, title: 'Unauthorized' }), { status: 401 }),
      ),
  );
  render(<App />);
  expect(await screen.findByRole('heading', { name: 'Sign in' })).toBeInTheDocument();
  expect(screen.queryByText('Checking the appliance…')).not.toBeInTheDocument();
  expect(expired).not.toHaveBeenCalled();
  window.removeEventListener('filedock:session-expired', expired);
});

for (const status of [502, 503])
  test(`startup ${status} shows retry rather than sign-in`, async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status })));
    render(<App />);
    expect(
      await screen.findByRole('heading', { name: 'Appliance unavailable' }),
    ).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Sign in' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry' })).toBeEnabled();
  });
test('network failure shows retry', async () => {
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Network unavailable')));
  render(<App />);
  expect(await screen.findByRole('heading', { name: 'Appliance unavailable' })).toBeInTheDocument();
});
