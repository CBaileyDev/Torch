import React, { useRef, useState } from 'react';
import { Torch } from './TorchSvg';
import { ModelPopover } from './ModelPopover';
import type { StageStatus } from '../store';
import type { StageSetting } from '../bridge/types';
import styles from './StageCard.module.css';

function fmtElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

interface Props {
  name: string;
  stage: StageStatus;
  isRunning?: boolean;
  onModelChange?: (model: string, effort: StageSetting['effort']) => void;
  children?: React.ReactNode;
}

export function StageCard({ name, stage, isRunning, onModelChange, children }: Props) {
  const [popoverOpen, setPopoverOpen] = useState(false);
  const chipRef = useRef<HTMLButtonElement>(null);

  const isLive = stage.torchState === 'lit';

  return (
    <div className={`${styles.module} ${isLive || isRunning ? styles.live : ''}`}>
      <div className={styles.modTop}>
        <span className={styles.modName}>{name}</span>
        <span className={styles.modTorch}>
          <Torch width={22} height={33} state={stage.torchState} />
        </span>
      </div>

      <div className={styles.chipRow}>
        <button
          ref={chipRef}
          className={styles.modModel}
          onClick={() => setPopoverOpen((o) => !o)}
          aria-haspopup="dialog"
          aria-expanded={popoverOpen}
          title="Edit model and effort"
        >
          {stage.model}
        </button>
        <span className={styles.modEffort}>{stage.effort}</span>

        {popoverOpen && onModelChange && (
          <div className={styles.popoverAnchor}>
            <ModelPopover
              model={stage.model}
              effort={stage.effort}
              onClose={() => setPopoverOpen(false)}
              onChange={(m, e) => { onModelChange(m, e); setPopoverOpen(false); }}
              anchorRef={chipRef as React.RefObject<HTMLElement | null>}
            />
          </div>
        )}
      </div>

      {children}

      <div className={styles.modStat}>
        <span className={`${styles.statLabel} ${isLive ? styles.statLabelLive : ''}`}>
          {stage.statusText}
        </span>
        <span className={styles.statTime}>
          {stage.elapsed > 0 ? fmtElapsed(stage.elapsed) : (stage.turns > 0 ? `${stage.turns}t` : '—')}
        </span>
      </div>
    </div>
  );
}
