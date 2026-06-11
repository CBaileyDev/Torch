import type { Bridge } from './types';
import { demoBridge } from './demo';

// Tauri is loaded dynamically to avoid import errors in browser mode.
// We do a runtime check and dynamic import at startup.
let bridge: Bridge = demoBridge;

export async function initBridge(): Promise<Bridge> {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    const { tauriBridge } = await import('./tauri');
    bridge = tauriBridge;
  }
  return bridge;
}

export function getBridge(): Bridge {
  return bridge;
}

export type { Bridge };
export * from './types';
