import type { FormEvent } from 'react';

type Props = {
  recoveryKey: string;
  keyId: string;
  confirmGroupIndexes: number[];
  onConfirm: (groups: Record<number, string>) => Promise<void>;
  error?: string;
};

export function RecoveryKeyStep({ recoveryKey, confirmGroupIndexes, onConfirm, error }: Props) {
  const groups = recoveryKey.split('-');
  const values: Record<number, string> = {};

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const next: Record<number, string> = {};
    for (const index of confirmGroupIndexes) {
      next[index] = String(data.get(`g${index}`) ?? '');
    }
    await onConfirm(next);
  }

  return (
    <section className="panel stack">
      <p className="eyebrow">Recovery key</p>
      <h2>Write this down once</h2>
      <p className="warn">
        If this appliance is lost and you do not have this key, the appliance state cannot be
        recovered.
      </p>
      <p className="recovery-key" data-testid="recovery-key">
        {recoveryKey}
      </p>
      <p>Confirm groups {confirmGroupIndexes.map((i) => i + 1).join(' and ')} to finish setup.</p>
      <form className="stack" onSubmit={submit}>
        {confirmGroupIndexes.map((index) => (
          <label key={index}>
            Group {index + 1}
            <input
              name={`g${index}`}
              defaultValue={values[index]}
              autoComplete="off"
              spellCheck={false}
              placeholder={groups[index] ? '•••••' : ''}
              required
            />
          </label>
        ))}
        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}
        <button className="primary" type="submit">
          Confirm recovery key
        </button>
      </form>
    </section>
  );
}
