import { useState } from 'react';

type Props = {
  recoveryKey: string;
  onDownloaded: () => Promise<void>;
  error?: string;
};

export function RecoveryKeyStep({ recoveryKey, onDownloaded, error }: Props) {
  const [busy, setBusy] = useState(false);

  async function downloadAndContinue() {
    if (busy) return;
    setBusy(true);
    const url = URL.createObjectURL(
      new Blob([`${recoveryKey}\n`], { type: 'text/plain;charset=utf-8' }),
    );
    try {
      const link = document.createElement('a');
      link.href = url;
      link.download = 'filedock-recovery-key.txt';
      document.body.appendChild(link);
      link.click();
      link.remove();
      await onDownloaded();
    } finally {
      // Let the browser start reading the Blob before releasing its URL.
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      setBusy(false);
    }
  }

  return (
    <section className="panel stack">
      <p className="eyebrow">Recovery key</p>
      <h2>Download your recovery key</h2>
      <p className="warn">
        If this appliance is lost and you do not have this key, the appliance state cannot be
        recovered.
      </p>
      <p className="recovery-key" data-testid="recovery-key">
        {recoveryKey}
      </p>
      <p>Keep the downloaded text file somewhere safe.</p>
      {error ? (
        <p className="error" role="alert">
          {error}
        </p>
      ) : null}
      <button className="primary" type="button" disabled={busy} onClick={downloadAndContinue}>
        {busy ? 'Finishing setup…' : 'Download recovery key and continue'}
      </button>
    </section>
  );
}
