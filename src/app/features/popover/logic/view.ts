import type { Status } from '@app/services/kairos/kairos-api';

export type ViewName = 'start' | 'running' | 'paused' | 'switch';

/** L'écran à montrer. « Changer de lot » n'a de sens que si un lot existe (en cours ou en pause). */
export function pickView(status: Status, switching: boolean): ViewName {
  if (status.state === 'idle') {
    return 'start';
  }
  return switching ? 'switch' : status.state;
}

const PALETTE_SIZE = 6;

/** Un entier de 0 à 5 stable pour un parent (ID, à défaut son nom) : sa couleur dans les listes. */
export function parentColorIndex(key: string): number {
  let hash = 0;
  for (const char of key) {
    hash = (hash * 31 + char.codePointAt(0)!) >>> 0;
  }
  return hash % PALETTE_SIZE;
}
