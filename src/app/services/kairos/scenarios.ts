import type { BridgeError, DayView, EntryView, LotView, RecentLot, Snapshot, Source, Status } from './kairos-api';

export const SCENARIOS = [
  'idle',
  'running',
  'paused',
  'switching',
  'error-locked',
  'error-newer',
  'no-id',
  'long-names',
  'no-recent',
  'claude-launched',
  'quit',
] as const;
export type ScenarioName = (typeof SCENARIOS)[number];

export const isScenario = (value: string | null): value is ScenarioName => SCENARIOS.some((name) => name === value);

/** Un scénario : l'état de départ du faux pont, et les refus qu'il oppose ensuite. */
interface Scenario {
  snapshot: Snapshot;
  /** Refus rendu à tous les appels suivant le premier `snapshot` (base verrouillée ou trop récente). */
  failure?: BridgeError;
}

const HOUR = 3600;
const MOCK_HOME = '/Users/lucas';
const LOCKED: BridgeError = { code: 'db_unavailable', message: "La base de Kairos est inaccessible : un autre programme l'utilise. Réessaie dans un instant." };
const NEWER: BridgeError = { code: 'db_newer', message: "La base de Kairos a été écrite par une version plus récente de l'app : mets l'app à jour." };

const lot = (
  id: number,
  parent: string | null,
  subtask: string | null,
  label: string | null = null,
  name: string | null = null,
  subtaskId: string | null = null,
): LotView => ({
  id,
  parent_monday_id: parent,
  parent_label: label,
  subtask,
  subtask_monday_id: subtaskId,
  name,
});

const entry = (id: number, l: LotView, start: number, end: number | null): EntryView => ({ id, lot: l, start, end, removed: false });

/** Le jour affiché : ses entrées, et ses bornes et totaux recalculés d'après elles. */
export function dayOf(entries: EntryView[], now: number): DayView {
  const closed = entries.filter((e) => !e.removed);
  const total = closed.reduce((sum, e) => sum + ((e.end ?? now) - e.start), 0);
  const pauses = closed.slice(1).reduce((sum, e, i) => sum + Math.max(0, e.start - (closed[i].end ?? now)), 0);
  const date = new Date(now * 1000);
  const iso = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
  return {
    date: iso,
    status: 'open',
    entries,
    start: closed[0]?.start ?? null,
    end: closed.length && closed[closed.length - 1].end ? closed[closed.length - 1].end : null,
    pauses,
    total,
  };
}

function snapshotOf(status: Status, entries: EntryView[], recent: RecentLot[], now: number, own: string[] = ['Recette']): Snapshot {
  return {
    status,
    today: dayOf(entries, now),
    recent,
    choices: { main: 'Front', base: ['Back', 'Design', 'Réunion'], own },
    server_time: now,
    home: MOCK_HOME,
  };
}

function recents(lots: LotView[], now: number, currentId: number | null): RecentLot[] {
  return lots.map((l, i) => ({ lot: l, last_end: now - (i + 1) * HOUR, today_total: (i + 1) * 900, current: l.id === currentId }));
}

/** Les écrans-types, relatifs à `now` (secondes) pour que les heures restent plausibles. */
export function scenario(name: ScenarioName, now: number): Scenario {
  const refonte = lot(1, '12406850311', 'Front', 'Refonte du tableau de bord', null, '12406850412');
  const api = lot(2, '98765432100', 'Back', 'API de facturation');
  const noId = lot(3, null, 'Front', null, 'Réunion de cadrage');
  const long = lot(
    4,
    '12406850311',
    'Intégration des maquettes et revue de code avec toute l’équipe produit',
    "Refonte complète du tableau de bord d'activité des équipes commerciales internationales",
  );
  const earlier = [entry(10, api, now - 5 * HOUR, now - 3 * HOUR), entry(11, noId, now - 3 * HOUR + 600, now - 2 * HOUR)];
  const running = (l: LotView, by: Source | null, workdir: string | null): Snapshot => {
    const since = now - 2533;
    const status: Status = { state: 'running', lot: l, since, duration: 2533, today_total: 2533 + 3 * HOUR, launched_by: by, workdir };
    return snapshotOf(status, [...earlier, entry(12, l, since, null)], recents([refonte, api, noId], now, l.id), now);
  };

  switch (name) {
    case 'idle':
      return { snapshot: snapshotOf({ state: 'idle' }, earlier, recents([refonte, api, noId], now, null), now) };
    case 'running':
    case 'switching':
    case 'quit':
      return { snapshot: running(refonte, 'manual', null) };
    case 'claude-launched':
      return { snapshot: running(refonte, 'claude-code', `${MOCK_HOME}/Beeside/main`) };
    case 'error-locked':
      return { snapshot: running(refonte, 'manual', null), failure: LOCKED };
    case 'error-newer':
      return { snapshot: running(refonte, 'manual', null), failure: NEWER };
    case 'no-id':
      return { snapshot: running(noId, 'manual', null) };
    case 'long-names':
      return { snapshot: running(long, 'claude-desktop', `${MOCK_HOME}/Documents/un-dossier-au-nom-tres-long/et-son-sous-dossier-encore-plus-long`) };
    case 'no-recent':
      return { snapshot: snapshotOf({ state: 'idle' }, [], [], now) };
    case 'paused': {
      const status: Status = { state: 'paused', lot: refonte, since: now - 600, duration: 2533, launched_by: 'manual', workdir: null };
      return {
        snapshot: snapshotOf(status, [...earlier, entry(12, refonte, now - 600 - 2533, now - 600)], recents([refonte, api, noId], now, refonte.id), now),
      };
    }
  }
}
