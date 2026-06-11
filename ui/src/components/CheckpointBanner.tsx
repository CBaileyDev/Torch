import styles from './CheckpointBanner.module.css';

interface Props {
  nextStage: string;
  onApprove: () => void;
  onReject: () => void;
}

export function CheckpointBanner({ nextStage, onApprove, onReject }: Props) {
  return (
    <div className={styles.banner} role="alert" aria-live="polite">
      <div className={styles.content}>
        <span className={styles.label}>Checkpoint</span>
        <span className={styles.message}>
          Spec ready — review the artifact before the implementer writes files.
          Next stage: <span className={styles.stage}>{nextStage}</span>
        </span>
      </div>
      <div className={styles.actions}>
        <button className={styles.rejectBtn} onClick={onReject}>
          Reject
        </button>
        <button className={styles.approveBtn} onClick={onApprove}>
          Approve
        </button>
      </div>
    </div>
  );
}
