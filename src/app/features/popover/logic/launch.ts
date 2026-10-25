import type { Source } from '@app/services/kairos/kairos-api';

export interface LaunchInfo {
  /** Qui a lancé l'entrée, tel que le cœur l'a écrit (une valeur inconnue est tolérée). */
  source: Source | string | null | undefined;
  /** Début de l'entrée, en secondes. */
  startedAt: number;
  workdir: string | null | undefined;
  home: string;
  /** Met en forme l'heure de `startedAt` (« 13:34 »), injectée pour garder ce module pur. */
  hm: (seconds: number) => string;
}

/** Les morceaux de « Lancé par Claude Code à 13:34 · ~/Beeside/main », que la vue traduit. */
export interface LaunchParts {
  /** `null` : lancé à la main, ou par une source inconnue (la vue dit alors « Lancé à … »). */
  who: 'claude-code' | 'claude-desktop' | null;
  time: string;
  folder: string | null;
}

/** Les deux derniers composants d'un chemin, séparés par « / ». */
const lastTwo = (parts: string[]): string => parts.slice(-2).join('/');

/** « ~/Beeside/main » sous le dossier personnel, « …/b/c » ailleurs. */
export function shortWorkdir(workdir: string, home: string): string {
  const path = workdir.replace(/\/+$/, '');
  const root = home.replace(/\/+$/, '');
  if (root && path === root) {
    return '~';
  }
  if (root && path.startsWith(`${root}/`)) {
    return `~/${lastTwo(path.slice(root.length + 1).split('/'))}`;
  }
  return `…/${lastTwo(path.split('/').filter(Boolean))}`;
}

export function describeLaunch({ source, startedAt, workdir, home, hm }: LaunchInfo): LaunchParts {
  const who = source === 'claude-code' || source === 'claude-desktop' ? source : null;
  return { who, time: hm(startedAt), folder: workdir ? shortWorkdir(workdir, home) : null };
}
