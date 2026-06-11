import { Torch } from './TorchSvg';
import styles from './Titlebar.module.css';

// One window chrome, not two: the native titlebar overlays this bar
// (titleBarStyle Overlay + hiddenTitle), so we only leave room for the
// macOS traffic lights and provide the drag region ourselves.
const inTauri = '__TAURI_INTERNALS__' in window;

export function Titlebar() {
  return (
    <div
      className={`${styles.titlebar} ${inTauri ? styles.trafficLightInset : ''}`}
      data-tauri-drag-region
    >
      <div className={styles.wordmark} data-tauri-drag-region>
        <span className={styles.torchGlyph}>
          <Torch width={16} height={24} state="lit" />
        </span>
        <span className={styles.wordmarkText} data-tauri-drag-region>
          TORCH
        </span>
      </div>
    </div>
  );
}
