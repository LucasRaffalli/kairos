import type { Source } from '@app/services/kairos/kairos-api';

export interface LaunchInfo {
  source: Source | null | undefined;
  /** Début de l'entrée, en secondes. */
  startedAt: number;
  workdir: string | null | undefined;
  home: string;
  /** Met en forme l'heure de `startedAt` (« 13:34 »), injectée pour garder ce module pur. */
  hm: (seconds: number) => string;
}

const WHO: Record<Source, string> = { manual: '', 'claude-code': ' par Claude Code', 'claude-desktop': ' par Claude Desktop' };

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

/** « Lancé par Claude Code à 13:34 · ~/Beeside/main » ; « Lancé à 13:34 » pour un lancement à la main. */
export function describeLaunch({ source, startedAt, workdir, home, hm }: LaunchInfo): string {
  const text = `Lancé${WHO[source ?? 'manual']} à ${hm(startedAt)}`;
  return workdir ? `${text} · ${shortWorkdir(workdir, home)}` : text;
}
