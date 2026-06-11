import { useEffect, useRef } from 'react';
import { useAppStore } from './store';
import { initBridge, getBridge } from './bridge';
import type { ProviderId } from './bridge/types';
import { Titlebar } from './components/Titlebar';
import { RunsSidebar } from './components/RunsSidebar';
import { ControlRoom } from './components/ControlRoom';
import { SettingsPanel } from './components/SettingsPanel';
import './styles/tokens.css';
import './styles/global.css';
import styles from './App.module.css';

export default function App() {
  const theme = useAppStore((s) => s.theme);
  const settingsOpen = useAppStore((s) => s.settingsOpen);
  const activeRunId = useAppStore((s) => s.activeRunId);
  const runs = useAppStore((s) => s.runs);
  const store = useAppStore();

  const tickRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const bridgeInitialized = useRef(false);

  useEffect(() => {
    if (bridgeInitialized.current) return;
    bridgeInitialized.current = true;

    void (async () => {
      const bridge = await initBridge();

      // Load settings
      const savedSettings = await bridge.getSettings();
      if (savedSettings['theme']) {
        const t = savedSettings['theme'] as typeof theme;
        store.setTheme(t);
      }
      if (savedSettings['plan_tier']) {
        const { planTier, ...rest } = store.settings;
        void planTier;
        store.setSettings({ ...rest, planTier: savedSettings['plan_tier'] as 'pro' | 'max5x' | 'max20x' });
      }

      // Restore escalation settings if persisted
      const escalationPatch: Partial<typeof store.settings> = {};
      if (savedSettings['escalation_provider']) {
        escalationPatch.escalationProvider = savedSettings['escalation_provider'] as ProviderId;
      }
      if (savedSettings['escalation_model']) {
        escalationPatch.escalationModel = savedSettings['escalation_model'];
      }
      if (savedSettings['escalation_effort']) {
        escalationPatch.escalationEffort = savedSettings['escalation_effort'] as typeof store.settings.escalationEffort;
      }
      if (Object.keys(escalationPatch).length > 0) {
        store.setSettings(escalationPatch);
      }

      // Restore per-stage providers if persisted
      const stageKeys = ['intake', 'plan', 'critic_a', 'critic_b', 'merge', 'implement', 'refine'] as const;
      const restoredProviders: Record<string, ProviderId> = { ...store.settings.stageProviders };
      let providerPatched = false;
      for (const key of stageKeys) {
        const v = savedSettings[`stage_provider_${key}`];
        if (v) {
          restoredProviders[key] = v as ProviderId;
          providerPatched = true;
        }
      }
      if (providerPatched) {
        store.setSettings({ stageProviders: restoredProviders });
      }

      // Probe installed provider CLIs FIRST — fast filesystem lookups that
      // populate the provider dropdowns. Must not be blocked by the slow
      // live claude-model probe below.
      try {
        const providerInfos = await bridge.probeProviders();
        store.setProviders(providerInfos);
      } catch (e) {
        console.error('probeProviders failed', e);
      }

      // Load history (fast, independent of model probing)
      try {
        const summaries = await bridge.listRuns();
        store.setHistorySummaries(summaries);
      } catch (e) {
        console.error('listRuns failed', e);
      }

      // Probe claude model availability LAST and in the background — it
      // makes live `claude` calls and can take 10–20s on a cold cache.
      // The dropdowns already work without it via provider suggestions.
      void bridge
        .probeModels()
        .then((models) => {
          if (models.length > 0) store.setAvailableModels(models);
        })
        .catch((e) => console.error('probeModels failed', e));

      // Subscribe to events
      const unsubEngine = bridge.onEngineEvent((envelope) => {
        store.applyEvent(envelope);
      });

      const unsubFailed = bridge.onRunFailed((payload) => {
        store.failRun(payload.runId, payload.error);
      });

      // Elapsed tick
      tickRef.current = setInterval(() => {
        store.tickElapsed();
      }, 1000);

      return () => {
        unsubEngine();
        unsubFailed();
        if (tickRef.current) clearInterval(tickRef.current);
      };
    })();
  }, [store]);

  const handleSelectRun = (id: string) => {
    // If it's the currently running run, just switch focus
    store.setActiveRun(id);
    // If it's a history run without in-memory state, load events
    if (!runs[id]) {
      void (async () => {
        const envelopes = await getBridge().getRunEvents(id);
        // Find summary info
        const summary = store.historySummaries.find((s) => s.id === id);
        if (!summary) return;
        // Reconstruct a run config from defaults for display
        const { settings } = store;
        const config = {
          goal: summary.goal,
          workdir: summary.workdir,
          preset: summary.preset as 'standard' | 'classic_linear' | 'fast',
          intake:    { provider: (settings.stageProviders['intake']   ?? 'claude') as ProviderId, model: settings.stageModels['intake']   ?? 'sonnet', effort: settings.stageEfforts['intake']   ?? 'low' },
          plan:      { provider: (settings.stageProviders['plan']      ?? 'claude') as ProviderId, model: settings.stageModels['plan']      ?? 'fable',  effort: settings.stageEfforts['plan']      ?? 'max' },
          critic_a:  { provider: (settings.stageProviders['critic_a']  ?? 'claude') as ProviderId, model: settings.stageModels['critic_a']  ?? 'opus',   effort: settings.stageEfforts['critic_a']  ?? 'high' },
          critic_b: settings.ensembleCritic
            ? { provider: (settings.stageProviders['critic_b'] ?? 'claude') as ProviderId, model: settings.stageModels['critic_b'] ?? 'fable', effort: settings.stageEfforts['critic_b'] ?? 'high' }
            : null,
          merge:     { provider: (settings.stageProviders['merge']     ?? 'claude') as ProviderId, model: settings.stageModels['merge']     ?? 'sonnet', effort: settings.stageEfforts['merge']     ?? 'medium' },
          implement: { provider: (settings.stageProviders['implement'] ?? 'claude') as ProviderId, model: settings.stageModels['implement'] ?? 'sonnet', effort: settings.stageEfforts['implement'] ?? 'medium' },
          refine:    { provider: (settings.stageProviders['refine']    ?? 'claude') as ProviderId, model: settings.stageModels['refine']    ?? 'sonnet', effort: settings.stageEfforts['refine']    ?? 'medium' },
          escalation: {
            provider: settings.escalationProvider,
            model: settings.escalationModel,
            effort: settings.escalationEffort,
          },
          max_refine_iterations: settings.maxRefineIterations,
          escalate_after: settings.escalateAfter,
          checkpoint_before_implement: true,
          verify_commands: [],
        };
        store.createRun(id, summary.goal, summary.workdir, config);
        // Replay all events
        for (const envelope of envelopes) {
          store.applyEvent(envelope);
        }
        // Mark as historical (not actively running)
        useAppStore.setState((s) => {
          const r = s.runs[id];
          if (!r) return s;
          return {
            runs: {
              ...s.runs,
              [id]: { ...r, status: summary.status as typeof r.status },
            },
          };
        });
      })();
    }
  };

  const activeRun = activeRunId ? (runs[activeRunId] ?? null) : null;

  return (
    <div className={`${styles.appRoot} ${theme}`}>
      <Titlebar />
      <div className={styles.layout}>
        <RunsSidebar onSelectRun={handleSelectRun} />
        <ControlRoom run={activeRun} onSettingsClick={() => store.setSettingsOpen(true)} />
      </div>
      {settingsOpen && <SettingsPanel onClose={() => store.setSettingsOpen(false)} />}
    </div>
  );
}
