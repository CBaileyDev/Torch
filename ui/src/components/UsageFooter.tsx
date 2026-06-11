import { useAppStore } from '../store';
import type { RunState } from '../store';
import styles from './UsageFooter.module.css';

function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

interface Props {
  run: RunState;
}

/// All stages share one Claude subscription's rate limits — this footer
/// keeps that spend visible at all times.
export function UsageFooter({ run }: Props) {
  const planTier = useAppStore((s) => s.settings.planTier);

  return (
    <footer className={styles.footer}>
      <span className={styles.item}>
        turns <b className={styles.value}>{run.totalTurns}</b>
      </span>
      <span className={styles.item}>
        out <b className={styles.value}>{fmtTokens(run.totalOutputTokens)} tok</b>
      </span>
      <span className={styles.item}>
        loop{' '}
        <b className={styles.value}>
          {run.loop.iteration}/{run.loop.maxIterations}
        </b>
      </span>
      <span className={styles.spacer} />
      <span className={styles.note}>all stages share your Claude subscription limits</span>
      <span className={styles.item}>
        tier <b className={styles.tier}>{planTier}</b>
      </span>
    </footer>
  );
}
