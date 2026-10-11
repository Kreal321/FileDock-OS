import { useState, type FormEvent } from 'react';
import { ApiError, api } from '../../api/client.ts';

export function SignInPage({ onSignedIn }: { onSignedIn: () => void }) {
  const [error, setError] = useState('');

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
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
    }
  }

  return (
    <form className="panel stack" onSubmit={submit}>
      <p className="eyebrow">Session</p>
      <h2>Sign in</h2>
      <label>
        Username
        <input name="username" autoComplete="username" required />
      </label>
      <label>
        Password
        <input name="password" type="password" autoComplete="current-password" required />
      </label>
      {error ? (
        <p className="error" role="alert">
          {error}
        </p>
      ) : null}
      <button className="primary" type="submit">
        Sign in
      </button>
    </form>
  );
}
