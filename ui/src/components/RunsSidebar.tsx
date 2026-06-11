import { useAppStore } from '../store';
import type { RunSummary } from '../bridge/types';
import styles from './RunsSidebar.module.css';

const STATUS_DOT: Record<RunSummary['status'], string> = {
  running: 'dotAmber',
  waiting: 'dotAmber',
  green: 'dotGreen',
  not_green: 'dotRed',
  failed: 'dotRed',
  cancelled: 'dotDim',
};

function statusLine(summary: RunSummary): string {
  switch (summary.status) {
    case 'running':
      return 'running';
    case 'waiting':
      return 'waiting for you';
    case 'green':
      return `green · ${summary.total_turns} turns`;
    case 'not_green':
      return `not green · ${summary.refine_iterations} iters`;
    case 'failed':
      return `failed · ${summary.total_turns} turns`;
    case 'cancelled':
      return 'cancelled';
  }
}

interface Props {
  onSelectRun: (id: string) => void;
}

export function RunsSidebar({ onSelectRun }: Props) {
  const historySummaries = useAppStore((s) => s.historySummaries);
  const activeRunId = useAppStore((s) => s.activeRunId);

  return (
    <aside className={styles.sidebar} aria-label="Run history">
      <div className={styles.label}>Runs</div>
      <div className={styles.list}>
        {historySummaries.length === 0 && (
          <div className={styles.empty}>No runs yet — light the first torch.</div>
        )}
        {historySummaries.map((summary) => {
          const isActive = summary.id === activeRunId;
          return (
            <button
              key={summary.id}
              className={`${styles.item} ${isActive ? styles.itemActive : ''}`}
              onClick={() => onSelectRun(summary.id)}
              aria-current={isActive ? 'true' : undefined}
            >
              <span className={styles.goal}>{summary.goal}</span>
              <span className={styles.meta}>
                <i className={`${styles.dot} ${styles[STATUS_DOT[summary.status]]}`} />
                {statusLine(summary)}
              </span>
            </button>
          );
        })}
      </div>
      <div className={styles.footer}>torch v0.1.0</div>
    </aside>
  );
}
