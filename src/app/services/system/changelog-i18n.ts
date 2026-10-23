import { ChangelogEntry, ChangelogTranslation } from '@app/interfaces';

/** `changelog.en.json` : une traduction par version. */
export type ChangelogTranslations = Record<string, ChangelogTranslation>;

/**
 * Le journal dans une autre langue : le français, chaque texte remplacé par sa
 * traduction quand elle existe. Le repli est champ par champ : une note oubliée
 * reste lisible en français au milieu de l'anglais, là où un trou se lirait
 * comme une panne. La nature et la date ne se traduisent pas.
 *
 * Sans dépendance à Angular, pour être éprouvé par `scripts/verify`.
 */
export function localizeChangelog(entries: ChangelogEntry[], translations: ChangelogTranslations | null): ChangelogEntry[] {
  if (!translations) {
    return entries;
  }
  return entries.map((entry) => {
    const tr = translations[entry.version];
    if (!tr) {
      return entry;
    }
    return {
      ...entry,
      title: tr.title ?? entry.title,
      notes: entry.notes.map((note, index) => {
        const translated = tr.notes[index];
        return translated ? { ...note, text: translated.text, how: translated.how ?? note.how } : note;
      }),
    };
  });
}

/** Les versions les plus récentes d'abord. Compare numéro par numéro : 0.10.0 > 0.9.0. */
export function newestFirst(entries: ChangelogEntry[]): ChangelogEntry[] {
  const parts = (v: string) => v.split('.').map((n) => Number.parseInt(n, 10) || 0);
  return [...entries].sort((a, b) => {
    const [x, y] = [parts(a.version), parts(b.version)];
    for (let i = 0; i < Math.max(x.length, y.length); i++) {
      const diff = (y[i] ?? 0) - (x[i] ?? 0);
      if (diff !== 0) {
        return diff;
      }
    }
    return 0;
  });
}

/**
 * Faut-il annoncer les nouveautés ? Jamais au tout premier lancement (rien n'a
 * « changé » pour quelqu'un qui découvre l'app), puis une fois par version.
 */
export function shouldAnnounce(current: string, seen: string | null): boolean {
  return seen !== null && seen !== current;
}
