import type { DayView, LotView } from '@app/services/kairos/kairos-api';

/** Une ligne de la liste du jour. `open` : l'entrée en cours, dont la durée suit le chrono. */
export interface TodayRow {
  id: number;
  lot: LotView;
  start: number;
  end: number | null;
  seconds: number;
  open: boolean;
}

/**
 * Les entrées du jour à afficher, la plus récente d'abord (sans les supprimées), et leur total.
 * L'entrée ouverte compte `openSeconds` (le chrono vivant) au lieu d'une durée figée au relevé.
 */
export function todayRows(day: Pick<DayView, 'entries'>, openSeconds: number): { rows: TodayRow[]; total: number } {
  const rows = day.entries
    .filter((entry) => !entry.removed)
    .map((entry) => ({
      id: entry.id,
      lot: entry.lot,
      start: entry.start,
      end: entry.end,
      open: entry.end === null,
      seconds: entry.end === null ? openSeconds : Math.max(0, entry.end - entry.start),
    }))
    .sort((a, b) => b.start - a.start);
  return { rows, total: rows.reduce((sum, row) => sum + row.seconds, 0) };
}
