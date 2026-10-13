import { Field } from '../../components/Field.tsx';
import { Button } from '../../components/Button.tsx';
import { useState, type FormEvent } from 'react';
import { ApiError, api, hasCsrfCookie } from '../../api/client.ts';
import { RecoveryKeyStep } from './RecoveryKeyStep.tsx';

type Generated = {
  username: string;
  applianceName: string;
  keyId: string;
  recoveryKey: string;
};

export function SetupWizard({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState<'code' | 'admin' | 'key'>('code');
  const [code, setCode] = useState('');
  const [generated, setGenerated] = useState<Generated | null>(null);
  const [error, setError] = useState('');
  const [needsSignIn, setNeedsSignIn] = useState(false);
  const [signingIn, setSigningIn] = useState(false);
  const [busy, setBusy] = useState(false);

  async function verify(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) return;
    setError('');
    const form = new FormData(event.currentTarget);
    const next = String(form.get('code') ?? '');
    setBusy(true);
    try {
      await api.post('/setup/verify-code', { code: next });
      setCode(next);
      setStep('admin');
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Could not verify the setup code.');
    } finally {
      setBusy(false);
    }
  }

  async function complete(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) return;
    setError('');
    const form = new FormData(event.currentTarget);
    const password = String(form.get('adminPassword') ?? '');
    const passwordConfirmation = String(form.get('adminPasswordConfirmation') ?? '');
    if (password !== passwordConfirmation) {
      setError('Passwords do not match.');
      return;
    }
    setBusy(true);
    try {
      const result = await api.post<Generated>('/setup/complete', {
        code,
        applianceName: String(form.get('applianceName') ?? ''),
        adminUsername: String(form.get('adminUsername') ?? ''),
        adminPassword: password,
        adminPasswordConfirmation: passwordConfirmation,
      });
      setGenerated(result);
      setStep('key');
      if (!hasCsrfCookie()) {
        setNeedsSignIn(true);
        setError(
          'Your browser did not save the session cookie. Allow cookies for this appliance, then sign in below to continue.',
        );
      }
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Setup could not be completed.');
    } finally {
      setBusy(false);
    }
  }

  async function confirm() {
    if (!generated) {
      return;
    }
    setError('');
    try {
      await api.post('/setup/recovery-key/confirm', { keyId: generated.keyId });
      onDone();
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) {
        setNeedsSignIn(true);
        setError(
          'Your setup session is missing or expired. Sign in below to continue with this recovery key.',
        );
      } else {
        setError(
          err instanceof ApiError ? err.message : 'Could not finish setup. Please try again.',
        );
      }
    }
  }

  async function signIn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!generated || signingIn) return;
    const formElement = event.currentTarget;
    const password = String(new FormData(formElement).get('password') ?? '');
    setSigningIn(true);
    setError('');
    try {
      await api.post('/auth/sign-in', { username: generated.username, password });
      formElement.reset();
      if (!hasCsrfCookie()) {
        setError(
          'Your browser did not save the session cookie. Allow cookies for this appliance and try again.',
        );
        return;
      }
      setNeedsSignIn(false);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Could not sign in. Please try again.');
    } finally {
      setSigningIn(false);
    }
  }

  return (
    <div className="stack">
      {step === 'code' ? (
        <form className="panel stack" onSubmit={verify}>
          <p className="eyebrow">First run</p>
          <h1>Enter the one-time setup code</h1>
          <Field
            label="Setup code"
            name="code"
            autoComplete="one-time-code"
            required
            placeholder="7KQ4-M2XD"
          />
          {error ? (
            <p className="error" role="alert">
              {error}
            </p>
          ) : null}
          <Button variant="primary" type="submit" busy={busy}>
            Continue
          </Button>
        </form>
      ) : null}
      {step === 'admin' ? (
        <form className="panel stack" onSubmit={complete}>
          <p className="eyebrow">Identity</p>
          <h1>Name the appliance and create the first administrator</h1>
          <Field label="Appliance name" name="applianceName" required defaultValue="filedock" />
          <Field
            label="Administrator username"
            name="adminUsername"
            required
            autoComplete="username"
            defaultValue="root"
          />
          <Field
            label="Password"
            name="adminPassword"
            type="password"
            required
            autoComplete="new-password"
          />
          <p className="eyebrow">We recommend using more than 8 characters.</p>
          <Field
            label="Re-enter password"
            name="adminPasswordConfirmation"
            type="password"
            required
            autoComplete="new-password"
          />
          {error ? (
            <p className="error" role="alert">
              {error}
            </p>
          ) : null}
          <Button variant="primary" type="submit" busy={busy}>
            Create administrator
          </Button>
        </form>
      ) : null}
      {step === 'key' && generated ? (
        <div className="stack">
          <RecoveryKeyStep
            recoveryKey={generated.recoveryKey}
            onDownloaded={confirm}
            error={error}
            canContinue={!needsSignIn}
          />
          {needsSignIn ? (
            <form className="panel stack" onSubmit={signIn}>
              <h2>Sign in to continue setup</h2>
              <p>Sign in as {generated.username}. Your recovery key stays the same.</p>
              <Field
                label="Administrator password"
                name="password"
                type="password"
                required
                autoComplete="current-password"
              />
              <Button variant="primary" type="submit" busy={signingIn}>
                {signingIn ? 'Signing in…' : 'Sign in'}
              </Button>
            </form>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
