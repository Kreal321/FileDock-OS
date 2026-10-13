import { useState, type FormEvent } from 'react';
import { ApiError, api } from '../../api/client.ts';
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

  async function verify(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError('');
    const form = new FormData(event.currentTarget);
    const next = String(form.get('code') ?? '');
    try {
      await api.post('/setup/verify-code', { code: next });
      setCode(next);
      setStep('admin');
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Could not verify the setup code.');
    }
  }

  async function complete(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError('');
    const form = new FormData(event.currentTarget);
    const password = String(form.get('adminPassword') ?? '');
    const passwordConfirmation = String(form.get('adminPasswordConfirmation') ?? '');
    if (password !== passwordConfirmation) {
      setError('Passwords do not match.');
      return;
    }
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
    } catch (err) {
      setError(err instanceof ApiError ? err.message : 'Setup could not be completed.');
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
      setError(err instanceof ApiError ? err.message : 'Could not finish setup. Please try again.');
    }
  }

  return (
    <div className="stack">
      {step === 'code' ? (
        <form className="panel stack" onSubmit={verify}>
          <p className="eyebrow">First run</p>
          <h2>Enter the one-time setup code</h2>
          <label>
            Setup code
            <input name="code" autoComplete="one-time-code" required placeholder="7KQ4-M2XD" />
          </label>
          {error ? (
            <p className="error" role="alert">
              {error}
            </p>
          ) : null}
          <button className="primary" type="submit">
            Continue
          </button>
        </form>
      ) : null}
      {step === 'admin' ? (
        <form className="panel stack" onSubmit={complete}>
          <p className="eyebrow">Identity</p>
          <h2>Name the appliance and create the first administrator</h2>
          <label>
            Appliance name
            <input name="applianceName" required defaultValue="filedock" />
          </label>
          <label>
            Administrator username
            <input name="adminUsername" required autoComplete="username" defaultValue="root" />
          </label>
          <label>
            Password
            <input name="adminPassword" type="password" required autoComplete="new-password" />
          </label>
          <p className="eyebrow">We recommend using more than 8 characters.</p>
          <label>
            Re-enter password
            <input
              name="adminPasswordConfirmation"
              type="password"
              required
              autoComplete="new-password"
            />
          </label>
          {error ? (
            <p className="error" role="alert">
              {error}
            </p>
          ) : null}
          <button className="primary" type="submit">
            Create administrator
          </button>
        </form>
      ) : null}
      {step === 'key' && generated ? (
        <RecoveryKeyStep recoveryKey={generated.recoveryKey} onDownloaded={confirm} error={error} />
      ) : null}
    </div>
  );
}
