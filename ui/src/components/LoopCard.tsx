import React, { useRef, useState } from 'react';
import { Torch } from './TorchSvg';
import { ModelPopover } from './ModelPopover';
import type { LoopStatus } from '../store';
import type { StageSetting } from '../bridge/types';
import styles from './LoopCard.module.css';

function fmtElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

interface Props {
  loop: LoopStatus;
  onModelChange?: (model: string, effort: StageSetting['effort']) => void;
}

export function LoopCard({ loop, onModelChange }: Props) {
  const [popoverOpen, setPopoverOpen] = useState(false);
  const chipRef = useRef<HTMLButtonElement>(null);

  const isLive = loop.torchState === 'lit';

  return (
    <div className={`${styles.module} ${isLive ? styles.live : ''}`}>
      <span className={styles.loopbadge}>loop ≤{loop.maxIterations}</span>
      <div className={styles.modTop}>
        <span className={styles.modName}>Verify ⇄ Refine</span>
        <span className={styles.modTorch}>
          <Torch width={22} height={33} state={loop.torchState} />
        </span>
      </div>

      <div className={styles.chipRow}>
        <button
          ref={chipRef}
          className={styles.modModel}
          onClick={() => setPopoverOpen((o) => !o)}
          aria-haspopup="dialog"
          aria-expanded={popoverOpen}
          title="Edit refine model and effort"
        >
          {loop.model}{loop.escalatedModel ? ` → ${loop.escalatedModel}` : ''}
        </button>
        <span className={styles.modEffort}>{loop.effort}</span>
        {popoverOpen && onModelChange && (
          <div className={styles.popoverAnchor}>
            <ModelPopover
              model={loop.model}
              effort={loop.effort}
              onClose={() => setPopoverOpen(false)}
              onChange={(m, e) => { onModelChange(m, e); setPopoverOpen(false); }}
              anchorRef={chipRef as React.RefObject<HTMLElement | null>}
            />
          </div>
        )}
      </div>

      {loop.escalatedModel && (
        <div className={styles.escalateNote}>
          escalated → {loop.escalatedModel} (iter {loop.iteration})
        </div>
      )}

      <div className={styles.modStat}>
        <span className={`${styles.statLabel} ${isLive ? styles.statLabelLive : ''}`}>
          {loop.statusText}
        </span>
        <span className={styles.statTime}>
          {loop.iteration > 0 ? `iter ${loop.iteration}` : (loop.elapsed > 0 ? fmtElapsed(loop.elapsed) : '—')}
        </span>
      </div>
    </div>
  );
}
