import { StageCard } from './StageCard';
import { CriticCard } from './CriticCard';
import { LoopCard } from './LoopCard';
import type { RunState } from '../store';
import type { StageSetting } from '../bridge/types';
import styles from './PipelineRail.module.css';

interface Props {
  run: RunState;
  onStageModelChange: (stage: string, model: string, effort: StageSetting['effort']) => void;
}

export function PipelineRail({ run, onStageModelChange }: Props) {
  return (
    <div className={styles.rail}>
      <StageCard
        name="Intake"
        stage={run.intake}
        onModelChange={(m, e) => onStageModelChange('intake', m, e)}
      />
      <StageCard
        name="Planner"
        stage={run.plan}
        onModelChange={(m, e) => onStageModelChange('plan', m, e)}
      />
      <CriticCard
        critic={run.critic}
        onModelChangeA={(m, e) => onStageModelChange('critic_a', m, e)}
        onModelChangeB={(m, e) => onStageModelChange('critic_b', m, e)}
      />
      <StageCard
        name="Implement"
        stage={run.implement}
        onModelChange={(m, e) => onStageModelChange('implement', m, e)}
      >
        {run.implement.toolUseCount > 0 && (
          <div className={styles.toolUseCount}>
            {run.implement.toolUseCount} tool calls
          </div>
        )}
      </StageCard>
      <LoopCard
        loop={run.loop}
        onModelChange={(m, e) => onStageModelChange('refine', m, e)}
      />
    </div>
  );
}
