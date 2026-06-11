import React, { useEffect, useRef, useState } from 'react';
import { useAppStore } from '../store';
import type { StageSetting } from '../bridge/types';
import styles from './ModelPopover.module.css';

const EFFORTS: StageSetting['effort'][] = ['low', 'medium', 'high', 'xhigh', 'max'];

interface Props {
  model: string;
  effort: StageSetting['effort'];
  onClose: () => void;
  onChange: (model: string, effort: StageSetting['effort']) => void;
  anchorRef: React.RefObject<HTMLElement | null>;
}

export function ModelPopover({ model, effort, onClose, onChange, anchorRef }: Props) {
  const availableModels = useAppStore((s) => s.availableModels);
  const [draftModel, setDraftModel] = useState(model);
  const [draftEffort, setDraftEffort] = useState(effort);
  const popoverRef = useRef<HTMLDivElement>(null);

  // The model probe can lag the UI; keep the current value selectable.
  const models = availableModels.includes(model)
    ? availableModels
    : [model, ...availableModels];

  useEffect(() => {
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (popoverRef.current?.contains(target)) return;
      if (anchorRef.current?.contains(target)) return;
      onClose();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [onClose, anchorRef]);

  return (
    <div ref={popoverRef} className={styles.popover} role="dialog" aria-label="Model and effort">
      <label className={styles.field}>
        <span className={styles.fieldLabel}>model</span>
        <select value={draftModel} onChange={(e) => setDraftModel(e.target.value)}>
          {models.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      </label>
      <label className={styles.field}>
        <span className={styles.fieldLabel}>effort</span>
        <select
          value={draftEffort}
          onChange={(e) => setDraftEffort(e.target.value as StageSetting['effort'])}
        >
          {EFFORTS.map((e) => (
            <option key={e} value={e}>
              {e}
            </option>
          ))}
        </select>
      </label>
      <div className={styles.actions}>
        <button className={styles.cancelBtn} onClick={onClose}>
          Cancel
        </button>
        <button className={styles.applyBtn} onClick={() => onChange(draftModel, draftEffort)}>
          Apply
        </button>
      </div>
    </div>
  );
}
