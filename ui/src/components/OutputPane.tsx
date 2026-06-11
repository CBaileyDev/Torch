import { useEffect, useRef } from 'react';
import type { RunState } from '../store';
import styles from './OutputPane.module.css';

function paneFor(run: RunState, stage: string): { title: string; text: string; live: boolean } {
  switch (stage) {
    case 'intake':
      return { title: 'intake', text: run.intake.transcript, live: run.intake.torchState === 'lit' };
    case 'plan':
      return { title: 'plan', text: run.plan.transcript, live: run.plan.torchState === 'lit' };
    case 'critic':
    case 'merge':
      return {
        title: stage === 'merge' ? 'merge' : 'critic-a',
        text: run.critic.a.transcript,
        live: run.critic.a.torchState === 'lit',
      };
    case 'implement':
      return {
        title: 'implement',
        text: run.implement.transcript,
        live: run.implement.torchState === 'lit',
      };
    case 'loop':
      return { title: 'refine', text: run.loop.transcript, live: run.loop.torchState === 'lit' };
    default:
      return { title: stage, text: '', live: false };
  }
}

function Transcript({ title, text, live }: { title: string; text: string; live: boolean }) {
  const bodyRef = useRef<HTMLDivElement>(null);

  // Follow the stream while it's live.
  useEffect(() => {
    if (live && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
    }
  }, [text, live]);

  return (
    <div className={styles.pane}>
      <div className={styles.paneHeader}>
        <span className={styles.paneTitle}>{title}</span>
        {live && <span className={styles.liveTag}>streaming</span>}
      </div>
      <div ref={bodyRef} className={styles.paneBody}>
        {text ? (
          <pre className={styles.text}>{text}</pre>
        ) : (
          <span className={styles.placeholder}>{live ? 'thinking…' : 'no output yet'}</span>
        )}
        {live && <span className={styles.caret} aria-hidden="true" />}
      </div>
    </div>
  );
}

interface Props {
  run: RunState;
}

export function OutputPane({ run }: Props) {
  // Critics in parallel get a split view; everything else a single pane.
  if (run.splitOutput && run.critic.b) {
    return (
      <div className={styles.split}>
        <Transcript
          title="critic-a"
          text={run.critic.a.transcript}
          live={run.critic.a.torchState === 'lit'}
        />
        <Transcript
          title="critic-b"
          text={run.critic.b.transcript}
          live={run.critic.b.torchState === 'lit'}
        />
      </div>
    );
  }

  const pane = paneFor(run, run.activeOutputStage);
  return (
    <div className={styles.single}>
      <Transcript {...pane} />
      {run.activeOutputStage === 'loop' && run.loop.lastSummary && (
        <div className={styles.verifyBox}>
          <div className={styles.verifyLabel}>verify · iteration {run.loop.iteration}</div>
          <pre className={styles.verifyText}>{run.loop.lastSummary}</pre>
        </div>
      )}
      {run.failureError && <div className={styles.failure}>{run.failureError}</div>}
    </div>
  );
}
