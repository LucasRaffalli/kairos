/** Formats de durée et d'heure du chronomètre. Fonctions pures : aucune dépendance à Angular. */

const pad = (n: number): string => String(n).padStart(2, '0');

/** « HH:MM:SS » : le grand chrono. Jamais négatif (une horloge en retard donne 00:00:00). */
export function formatClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  return `${pad(Math.floor(total / 3600))}:${pad(Math.floor(total / 60) % 60)}:${pad(total % 60)}`;
}

/** « 0 h 42 » : les totaux de la liste du jour. */
export function formatHm(seconds: number): string {
  const minutes = Math.floor(Math.max(0, seconds) / 60);
  return `${Math.floor(minutes / 60)} h ${pad(minutes % 60)}`;
}

/**
 * « lun. 20 oct. » : le jour de l'en-tête, dans la langue de l'interface. `date` est le jour du
 * cœur (« AAAA-MM-JJ », lu comme un jour local : un décalage UTC le ferait glisser d'un jour) ;
 * sans relevé, le jour de l'horloge.
 */
export function formatDayHeader(date: string | undefined, language: string): string {
  const [year, month, day] = (date ?? '').split('-').map(Number);
  const shown = date ? new Date(year, month - 1, day) : new Date();
  return new Intl.DateTimeFormat(language, { weekday: 'short', day: 'numeric', month: 'short' }).format(shown);
}

/** « 13:34 » : l'heure locale d'un instant, en secondes depuis l'époque. */
export function formatTimeOfDay(seconds: number): string {
  const date = new Date(seconds * 1000);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

const HHMM = /^([01]?\d|2[0-3]):([0-5]\d)$/;

/**
 * « HH:MM » → secondes depuis l'époque, à cette heure locale du jour de `nowMs`. `null` si le
 * texte est mal formé. Un début dans le futur n'est PAS refusé ici : c'est le cœur qui juge.
 * `setHours` travaille en heure locale, donc un changement d'heure ne décale pas l'instant demandé.
 */
export function secondsFromHhmm(text: string, nowMs: number): number | null {
  const match = HHMM.exec(text.trim());
  if (!match) {
    return null;
  }
  const date = new Date(nowMs);
  date.setHours(Number(match[1]), Number(match[2]), 0, 0);
  return Math.floor(date.getTime() / 1000);
}
