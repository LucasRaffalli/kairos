/** La nature d'une note : ce qui est nouveau, amélioré ou réparé. */
export type ChangeKind = 'new' | 'better' | 'fixed';

export interface ChangeNote {
  kind: ChangeKind;
  text: string;
  /** « Comment l'essayer » : une phrase, facultative. */
  how?: string;
}

/** Une version du journal, dans `src/assets/changelog.json` (français). */
export interface ChangelogEntry {
  version: string;
  /** AAAA-MM-JJ. */
  date: string;
  title: string;
  notes: ChangeNote[];
}

/** La traduction d'une version, dans `changelog.en.json` : appariée aux notes par position. */
export interface ChangelogTranslation {
  title?: string;
  notes: { text: string; how?: string }[];
}
