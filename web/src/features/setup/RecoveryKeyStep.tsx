import { Button } from '../../components/Button.tsx';
import { useState } from 'react';

type Props = {
  recoveryKey: string;
  onDownloaded: () => Promise<void>;
  error?: string;
  canContinue?: boolean;
  busyLabel?: string;
};

export function RecoveryKeyStep({
  recoveryKey,
  onDownloaded,
  error,
  canContinue = true,
  busyLabel = 'Finishing setup…',
}: Props) {
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
      if (canContinue) await onDownloaded();
    } finally {
      // Let the browser start reading the Blob before releasing its URL.
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      setBusy(false);
    }
  }

  return (
    <section className="panel stack">
      <p className="eyebrow">Recovery key</p>
      <h1>Download your recovery key</h1>
      <p className="warn">
        If this appliance is lost and you do not have this key, the appliance state cannot be
        recovered.
      </p>
      <p className="recovery-key" data-testid="recovery-key" data-secret>
        {recoveryKey}
      </p>
      <p>Keep the downloaded text file somewhere safe.</p>
      {error ? (
        <p className="error" role="alert">
          {error}
        </p>
      ) : null}
      <Button variant="primary" type="button" busy={busy} onClick={downloadAndContinue}>
        {busy
          ? busyLabel
          : canContinue
            ? 'Download recovery key and continue'
            : 'Download recovery key'}
      </Button>
    </section>
  );
}
