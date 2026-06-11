import { useAppStore } from '../store';
import type { PlanTier, Preset, Theme } from '../store';
import { getBridge } from '../bridge';
import styles from './SettingsPanel.module.css';

const THEMES: Array<{ id: Theme; name: string }> = [
  { id: 'theme-coal', name: 'Coal' },
  { id: 'theme-pitch', name: 'Pitch' },
  { id: 'theme-iron', name: 'Iron' },
  { id: 'theme-ember', name: 'Ember' },
];

// Plan-tier defaults from docs/ipc-contract.md.
const TIER_DEFAULTS: Record<PlanTier, { ensembleCritic: boolean; maxRefineIterations: number; escalateAfter: number }> = {
  pro: { ensembleCritic: false, maxRefineIterations: 3, escalateAfter: 2 },
  max5x: { ensembleCritic: false, maxRefineIterations: 3, escalateAfter: 2 },
  max20x: { ensembleCritic: true, maxRefineIterations: 5, escalateAfter: 1 },
};

interface Props {
  onClose: () => void;
}

export function SettingsPanel({ onClose }: Props) {
  const store = useAppStore();
  const { settings, theme } = store;

  const save = (key: string, value: string) => {
    void getBridge().saveSetting(key, value);
  };

  const handleTheme = (next: Theme) => {
    store.setTheme(next);
    save('theme', next);
  };

  const handleTier = (tier: PlanTier) => {
    const defaults = TIER_DEFAULTS[tier];
    store.setSettings({ planTier: tier, ...defaults });
    save('plan_tier', tier);
    save('ensemble_critic', String(defaults.ensembleCritic));
    save('max_refine_iterations', String(defaults.maxRefineIterations));
    save('escalate_after', String(defaults.escalateAfter));
  };

  return (
    <div className={styles.scrim} onClick={onClose}>
      <div
        className={styles.panel}
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onClick={(e) => e.stopPropagation()}
      >
        <div className={styles.header}>
          <span className={styles.title}>Settings</span>
          <button className={styles.closeBtn} onClick={onClose} aria-label="Close settings">
            ✕
          </button>
        </div>

        <div className={styles.section}>
          <span className={styles.sectionLabel}>Theme</span>
          <div className={styles.themeRow}>
            {THEMES.map((t) => (
              <button
                key={t.id}
                className={`${styles.themeSwatch} ${t.id} ${theme === t.id ? styles.themeActive : ''}`}
                onClick={() => handleTheme(t.id)}
                aria-pressed={theme === t.id}
              >
                <span className={styles.swatchChip} />
                {t.name}
              </button>
            ))}
          </div>
        </div>

        <div className={styles.section}>
          <span className={styles.sectionLabel}>Plan</span>
          <div className={styles.row}>
            <label className={styles.rowLabel} htmlFor="settings-tier">
              Claude plan tier
            </label>
            <select
              id="settings-tier"
              value={settings.planTier}
              onChange={(e) => handleTier(e.target.value as PlanTier)}
            >
              <option value="pro">pro</option>
              <option value="max5x">max 5x</option>
              <option value="max20x">max 20x</option>
            </select>
          </div>
          <div className={styles.row}>
            <label className={styles.rowLabel} htmlFor="settings-preset">
              Default preset
            </label>
            <select
              id="settings-preset"
              value={settings.preset}
              onChange={(e) => {
                store.setSettings({ preset: e.target.value as Preset });
                save('default_preset', e.target.value);
              }}
            >
              <option value="standard">standard</option>
              <option value="classic_linear">classic linear</option>
              <option value="fast">fast</option>
            </select>
          </div>
        </div>

        <div className={styles.section}>
          <span className={styles.sectionLabel}>Pipeline</span>
          <div className={styles.row}>
            <label className={styles.rowLabel} htmlFor="settings-heavy">
              Heavy Mode (implementer → opus)
            </label>
            <input
              id="settings-heavy"
              type="checkbox"
              checked={settings.heavyMode}
              onChange={(e) => {
                store.setSettings({ heavyMode: e.target.checked });
                save('heavy_mode', String(e.target.checked));
              }}
            />
          </div>
          <div className={styles.row}>
            <label className={styles.rowLabel} htmlFor="settings-ensemble">
              Ensemble critics (opus + fable)
            </label>
            <input
              id="settings-ensemble"
              type="checkbox"
              checked={settings.ensembleCritic}
              onChange={(e) => {
                store.setSettings({ ensembleCritic: e.target.checked });
                save('ensemble_critic', String(e.target.checked));
              }}
            />
          </div>
          <div className={styles.row}>
            <label className={styles.rowLabel} htmlFor="settings-max-iter">
              Max refine iterations
            </label>
            <select
              id="settings-max-iter"
              value={settings.maxRefineIterations}
              onChange={(e) => {
                store.setSettings({ maxRefineIterations: Number(e.target.value) });
                save('max_refine_iterations', e.target.value);
              }}
            >
              {[1, 2, 3, 4, 5, 6, 7, 8].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </div>
          <div className={styles.row}>
            <label className={styles.rowLabel} htmlFor="settings-escalate">
              Escalate after N repeated failures
            </label>
            <select
              id="settings-escalate"
              value={settings.escalateAfter}
              onChange={(e) => {
                store.setSettings({ escalateAfter: Number(e.target.value) });
                save('escalate_after', e.target.value);
              }}
            >
              {[1, 2, 3].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className={styles.section}>
          <span className={styles.sectionLabel}>Verify commands</span>
          <textarea
            className={styles.verifyInput}
            rows={3}
            placeholder={'one command per line, e.g.\ncargo test\nnpm test'}
            value={settings.verifyCommands}
            onChange={(e) => {
              store.setSettings({ verifyCommands: e.target.value });
              save('verify_commands', e.target.value);
            }}
            aria-label="Verify commands, one per line"
          />
          <span className={styles.hint}>
            Leave empty to let the intake stage decide. The orchestrator runs these itself —
            zero tokens.
          </span>
        </div>
      </div>
    </div>
  );
}
