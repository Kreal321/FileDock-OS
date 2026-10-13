import { Field } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
import { useState, type FormEvent } from 'react';
import { ApiError, api } from '../../api/client.ts';

export function SignInPage({ onSignedIn }: { onSignedIn: () => void }) {
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError('');
    const form = new FormData(event.currentTarget);
    try {
      await api.post('/auth/sign-in', {
        username: String(form.get('username') ?? ''),
        password: String(form.get('password') ?? ''),
      });
      onSignedIn();
    } catch (err) {
      const problem = err instanceof ApiError ? err.problem : undefined;
      if (problem?.status === 429 || (err instanceof ApiError && err.status === 429)) {
        setError('This account is locked for 15 minutes after five failed sign-in attempts.');
      } else {
        setError(err instanceof ApiError ? err.message : 'Sign-in failed.');
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <form className="panel stack" onSubmit={submit}>
      <p className="eyebrow">Session</p>
      <h1>Sign in</h1>
      <Field label="Username" name="username" autoComplete="username" required />
      <Field
        label="Password"
        name="password"
        type="password"
        autoComplete="current-password"
        required
      />
      {error ? (
        <p className="error" role="alert">
          {error}
        </p>
      ) : null}
      <Button variant="primary" type="submit" busy={busy}>
        Sign in
      </Button>
    </form>
  );
}
