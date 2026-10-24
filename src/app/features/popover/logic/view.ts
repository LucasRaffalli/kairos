import type { LotView, RecentLot, Status } from '@app/services/kairos/kairos-api';

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

/** Le nom d'un lot à l'écran : le libellé du parent, à défaut le nom du lot sans ID, à défaut l'ID. */
export function lotTitle(lot: LotView): string {
  return lot.parent_label || lot.name || lot.parent_monday_id || '';
}

/** Les pastilles « Récents » : un parent par ID (les lots sans ID n'ont rien à remplir), au plus `limit`. */
export function recentParents(recent: RecentLot[], limit: number): { id: string; name: string }[] {
  const seen = new Map<string, string>();
  for (const { lot } of recent) {
    if (lot.parent_monday_id && !seen.has(lot.parent_monday_id)) {
      seen.set(lot.parent_monday_id, lot.parent_label ?? '');
    }
  }
  return [...seen].slice(0, limit).map(([id, name]) => ({ id, name }));
}

/** La liste « Reprendre » : les lots les plus récents, sans celui qui tourne déjà (y basculer serait refusé). */
export function quickResumes(recent: RecentLot[], limit: number): LotView[] {
  return recent
    .filter((item) => !item.current)
    .slice(0, limit)
    .map((item) => item.lot);
}
