import React, { useRef, useState } from 'react';
import { Torch } from './TorchSvg';
import { ModelPopover } from './ModelPopover';
import type { CriticStatus } from '../store';
import type { StageSetting } from '../bridge/types';
import styles from './CriticCard.module.css';

function fmtElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

interface Props {
  critic: CriticStatus;
  onModelChangeA?: (model: string, effort: StageSetting['effort']) => void;
  onModelChangeB?: (model: string, effort: StageSetting['effort']) => void;
}

export function CriticCard({ critic, onModelChangeA, onModelChangeB }: Props) {
  const [popoverOpenA, setPopoverOpenA] = useState(false);
  const [popoverOpenB, setPopoverOpenB] = useState(false);
  const chipRefA = useRef<HTMLButtonElement>(null);
  const chipRefB = useRef<HTMLButtonElement>(null);

  const { a, b } = critic;
  const isLive = a.torchState === 'lit' || b?.torchState === 'lit';

  // Overall torch: pick the most "active" state
  const overallState = (() => {
    if (a.torchState === 'lit' || b?.torchState === 'lit') return 'lit';
    if (a.torchState === 'guttered' || b?.torchState === 'guttered') return 'guttered';
    if (a.torchState === 'spent' && (!b || b.torchState === 'spent')) return 'spent';
    return a.torchState;
  })();

  const overallStatus = (() => {
    if (a.torchState === 'lit' || b?.torchState === 'lit') return 'running';
    if (a.torchState === 'spent' && (!b || b.torchState === 'spent')) return 'done';
    if (a.torchState === 'guttered' || b?.torchState === 'guttered') return 'failed';
    return 'queued';
  })();

  const elapsedA = a.elapsed;
  const elapsedB = b?.elapsed ?? 0;

  return (
    <div className={`${styles.module} ${isLive ? styles.live : ''}`}>
      <div className={styles.modTop}>
        <span className={styles.modName}>Critic</span>
        <span className={styles.modTorch}>
          <Torch width={22} height={33} state={overallState} />
        </span>
      </div>

      {/* Sub-strips for ensemble */}
      <div className={b ? styles.split : styles.single}>
        {/* Critic A */}
        <div className={styles.substrip}>
          <div className={styles.substripTop}>
            <div className={styles.chipRow}>
              <button
                ref={chipRefA}
                className={styles.modModel}
                onClick={() => setPopoverOpenA((o) => !o)}
                aria-haspopup="dialog"
                aria-expanded={popoverOpenA}
                title="Edit critic-a model and effort"
              >
                {a.model}
              </button>
              <span className={styles.modEffort}>{a.effort}</span>
              {popoverOpenA && onModelChangeA && (
                <div className={styles.popoverAnchor}>
                  <ModelPopover
                    model={a.model}
                    effort={a.effort}
                    onClose={() => setPopoverOpenA(false)}
                    onChange={(m, e) => { onModelChangeA(m, e); setPopoverOpenA(false); }}
                    anchorRef={chipRefA as React.RefObject<HTMLElement | null>}
                </div>
              )}
            </div>
          </div>
          <div className={`${styles.vu} ${a.torchState === 'lit' ? styles.vuLive : ''}`}>
            <i className={a.torchState === 'lit' ? styles.vuBarLive : styles.vuBar} />
          </div>
        </div>

        {/* Critic B (ensemble only) */}
        {b && (
          <div className={styles.substrip}>
            <div className={styles.substripTop}>
              <div className={styles.chipRow}>
                <button
                  ref={chipRefB}
                  className={styles.modModel}
                  onClick={() => setPopoverOpenB((o) => !o)}
                  aria-haspopup="dialog"
                  aria-expanded={popoverOpenB}
                  title="Edit critic-b model and effort"
                >
                  {b.model}
                </button>
                <span className={styles.modEffort}>{b.effort}</span>
                {popoverOpenB && onModelChangeB && (
                  <div className={styles.popoverAnchor}>
                    <ModelPopover
                      model={b.model}
                      effort={b.effort}
                      onClose={() => setPopoverOpenB(false)}
                      onChange={(m, e) => { onModelChangeB(m, e); setPopoverOpenB(false); }}
                      anchorRef={chipRefB as React.RefObject<HTMLElement | null>}
                  </div>
                )}
              </div>
            </div>
            <div className={`${styles.vu} ${b.torchState === 'lit' ? styles.vuLive : ''}`}>
              <i
                className={b.torchState === 'lit' ? styles.vuBarLive : styles.vuBar}
                style={{ animationDuration: '3.7s' }}
              />
            </div>
          </div>
        )}
      </div>

      <div className={styles.modStat}>
        <span className={`${styles.statLabel} ${isLive ? styles.statLabelLive : ''}`}>
          {overallStatus}
        </span>
        <span className={styles.statTime}>
          {elapsedA > 0 ? fmtElapsed(Math.max(elapsedA, elapsedB)) : '—'}
        </span>
      </div>
    </div>
  );
}
