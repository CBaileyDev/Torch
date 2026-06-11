import { useCallback, useEffect, useRef, useState } from 'react';
import { useAppStore } from '../store';
import type { AppSettings, RunState } from '../store';
import type { RunConfig, StageSetting } from '../bridge/types';
import { getBridge } from '../bridge';
import { PipelineRail } from './PipelineRail';
import { IntakePanel } from './IntakePanel';
import { CheckpointBanner } from './CheckpointBanner';
import { OutputPane } from './OutputPane';
import { UsageFooter } from './UsageFooter';
import styles from './ControlRoom.module.css';

function fmtElapsed(startedAt: number): string {
  const s = Math.floor((Date.now() - startedAt) / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

function buildRunConfig(goal: string, workdir: string, settings: AppSettings): RunConfig {
  const { stageModels, stageEfforts, ensembleCritic, maxRefineIterations, escalateAfter, preset, heavyMode, verifyCommands } = settings;
  return {
    goal,
    workdir: workdir || '~',
    preset,
    intake:   { model: stageModels['intake']   ?? 'sonnet', effort: stageEfforts['intake']   ?? 'low' },
    plan:     { model: stageModels['plan']      ?? 'fable',  effort: stageEfforts['plan']      ?? 'max' },
    critic_a: { model: stageModels['critic_a']  ?? 'opus',   effort: stageEfforts['critic_a']  ?? 'high' },
    critic_b: ensembleCritic
      ? { model: stageModels['critic_b'] ?? 'fable', effort: stageEfforts['critic_b'] ?? 'high' }
      : null,
    merge:     { model: stageModels['merge']     ?? 'sonnet', effort: stageEfforts['merge']     ?? 'medium' },
    implement: { model: heavyMode ? 'opus' : (stageModels['implement'] ?? 'sonnet'), effort: stageEfforts['implement'] ?? 'medium' },
    refine:    { model: stageModels['refine']    ?? 'sonnet', effort: stageEfforts['refine']    ?? 'medium' },
    escalation_model: 'fable',
    max_refine_iterations: maxRefineIterations,
    escalate_after: escalateAfter,
    checkpoint_before_implement: true,
    verify_commands: verifyCommands.split('\n').map((c) => c.trim()).filter(Boolean),
  };
}

interface Props {
  run: RunState | null;
  onSettingsClick: () => void;
}

export function ControlRoom({ run, onSettingsClick }: Props) {
  const store = useAppStore();
  const settings = useAppStore((s) => s.settings);
  const activeRunId = useAppStore((s) => s.activeRunId);

  const [goal, setGoal] = useState('');
  const [workdir, setWorkdir] = useState('');
  const [elapsedStr, setElapsedStr] = useState('0:00');
  const elapsedRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const isRunning = run ? (run.status === 'running' || run.status === 'waiting') : false;

  // Elapsed timer
  useEffect(() => {
    if (run && isRunning) {
      elapsedRef.current = setInterval(() => {
        setElapsedStr(fmtElapsed(run.startedAt));
      }, 1000);
      return () => { if (elapsedRef.current) clearInterval(elapsedRef.current); };
    } else {
      if (elapsedRef.current) clearInterval(elapsedRef.current);
      if (run) setElapsedStr(fmtElapsed(run.startedAt));
      return;
    }
  }, [run, isRunning]);

  const handlePickDir = async () => {
    const dir = await getBridge().pickDirectory();
    if (dir) setWorkdir(dir);
  };

  const handleRun = async () => {
    if (!goal.trim()) return;
    const config = buildRunConfig(goal, workdir, settings);
    const bridge = getBridge();
    const runId = await bridge.startRun(config);
    store.createRun(runId, goal, workdir, config);
    setGoal('');
  };

  const handleCancel = async () => {
    if (!activeRunId) return;
    await getBridge().cancelRun(activeRunId);
  };

  const handleIntakeSubmit = async (answers: string[]) => {
    if (!activeRunId) return;
    store.applyEvent({ runId: activeRunId, event: { kind: 'awaiting_intake_answers', questions: [] } });
    await getBridge().sendIntakeAnswers(activeRunId, answers);
    // Clear the awaiting flag optimistically
    useAppStore.setState((s) => {
      const r = s.runs[activeRunId];
      if (!r) return s;
      return { runs: { ...s.runs, [activeRunId]: { ...r, awaitingIntakeAnswers: false, status: 'running' } } };
    });
  };

  const handleCheckpointApprove = async () => {
    if (!activeRunId) return;
    await getBridge().checkpointDecision(activeRunId, true);
    useAppStore.setState((s) => {
      const r = s.runs[activeRunId];
      if (!r) return s;
      return { runs: { ...s.runs, [activeRunId]: { ...r, awaitingCheckpoint: false, status: 'running' } } };
    });
  };

  const handleCheckpointReject = async () => {
    if (!activeRunId) return;
    await getBridge().checkpointDecision(activeRunId, false);
    useAppStore.setState((s) => {
      const r = s.runs[activeRunId];
      if (!r) return s;
      return { runs: { ...s.runs, [activeRunId]: { ...r, awaitingCheckpoint: false, status: 'failed' } } };
    });
  };

  const handleStageModelChange = useCallback((stage: string, model: string, effort: StageSetting['effort']) => {
    // Only affects config for next run (or display of current run if not yet started)
    store.setSettings({
      stageModels: { ...settings.stageModels, [stage]: model },
      stageEfforts: { ...settings.stageEfforts, [stage]: effort },
    });
  }, [store, settings]);

  const displayWorkdir = run?.workdir ?? workdir;
  const displayPreset = run?.preset ?? settings.preset;
  const displayTier = settings.planTier;

  return (
    <main className={styles.main}>
      {/* Prompt bar */}
      <div className={styles.promptbar}>
        <input
          className={styles.prompt}
          type="text"
          value={isRunning ? (run?.goal ?? '') : goal}
          onChange={(e) => !isRunning && setGoal(e.target.value)}
          placeholder="What do you want to build?"
          readOnly={isRunning}
          onKeyDown={(e) => { if (e.key === 'Enter' && !isRunning) void handleRun(); }}
          aria-label="Build goal"
        />
        <button
          className={`${styles.runBtn} ${isRunning ? styles.cancelBtn : ''}`}
          onClick={isRunning ? handleCancel : handleRun}
          disabled={!isRunning && !goal.trim()}
          aria-label={isRunning ? 'Cancel run' : 'Run'}
        >
          {isRunning ? 'Cancel run' : 'Run'}
        </button>
        <button
          className={styles.gearBtn}
          onClick={onSettingsClick}
          aria-label="Open settings"
          title="Settings"
        >
          &#9881;
        </button>
      </div>

      {/* Meta row */}
      <div className={styles.metaRow}>
        <span>
          dir{' '}
          <button className={styles.dirBtn} onClick={!isRunning ? handlePickDir : undefined} aria-label="Pick working directory">
            <b>{displayWorkdir || '(pick directory)'}</b>
          </button>
        </span>
        <span>preset <b>{displayPreset}</b></span>
        <span>plan <b>{displayTier}</b></span>
        {(isRunning || run) && (
          <span>elapsed <b>{elapsedStr}</b></span>
        )}
      </div>

      {/* Pipeline rail — only when there is an active run */}
      {run && (
        <>
          <PipelineRail run={run} onStageModelChange={handleStageModelChange} />

          {/* Intake Q&A */}
          {run.awaitingIntakeAnswers && run.intakeQuestions.length > 0 && (
            <IntakePanel questions={run.intakeQuestions} onSubmit={handleIntakeSubmit} />
          )}

          {/* Checkpoint banner */}
          {run.awaitingCheckpoint && (
            <CheckpointBanner
              nextStage={run.checkpointNextStage}
              onApprove={handleCheckpointApprove}
              onReject={handleCheckpointReject}
            />
          )}

          {/* Output pane */}
          <OutputPane run={run} />

          {/* Usage footer */}
          <UsageFooter run={run} />
        </>
      )}

      {/* Empty state */}
      {!run && (
        <div className={styles.emptyState}>
          <div className={styles.emptyInner}>
            <span className={styles.emptyHint}>
              Enter a goal above and press Run to start a pipeline.
            </span>
          </div>
        </div>
      )}
    </main>
  );
}
