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
