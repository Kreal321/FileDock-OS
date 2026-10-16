import { useState } from 'react';
import { Button } from '../../components/Button.tsx';
export function useResourcePage() {
  const [tokens, setTokens] = useState<string[]>(['']);
  const cursor = tokens.at(-1) ?? '';
  const query = `?pageSize=50&pageToken=${encodeURIComponent(cursor)}`;
  const controls = (next?: string | null) => (
    <nav className="row" aria-label="Resource pages">
      <Button
        variant="secondary"
        disabled={tokens.length < 2}
        onClick={() => setTokens((t) => t.slice(0, -1))}
      >
        Previous
      </Button>
      <Button
        variant="secondary"
        disabled={!next}
        onClick={() => next && setTokens((t) => [...t, next])}
      >
        Next
      </Button>
    </nav>
  );
  return { cursor, query, controls };
}
