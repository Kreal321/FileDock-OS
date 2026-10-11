import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState } from 'react';
import { api } from './api/client.ts';

const queryClient = new QueryClient();

function Shell() {
  const [page] = useState('dashboard');
  return (
    <div className="app">
      <header>
        <h1>FileDock</h1>
        <nav aria-label="Primary">
          <span>{page}</span>
        </nav>
      </header>
      <main>
        <p>The management UI shell is ready. Feature pages land with each user story.</p>
      </main>
    </div>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <Shell />
    </QueryClientProvider>
  );
}

void api;
