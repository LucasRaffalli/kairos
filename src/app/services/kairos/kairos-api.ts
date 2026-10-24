import { InjectionToken } from '@angular/core';

/** Qui a lancé le lot (sérialisé en kebab-case par le cœur). */
export type Source = 'manual' | 'claude-code' | 'claude-desktop';

export type DayStatus = 'open' | 'closed' | 'declared' | 'to_redeclare';

/** Un lot, avec ce qu'il faut pour l'afficher. */
export interface LotView {
  id: number;
  parent_monday_id: string | null;
  parent_label: string | null;
  subtask: string | null;
  name: string | null;
}

export type Status =
  | { state: 'idle' }
  | {
      state: 'running';
      lot: LotView;
      since: number;
      duration: number;
      today_total: number;
      launched_by: Source | null;
      workdir: string | null;
    }
  | {
      state: 'paused';
      lot: LotView;
      /** Début de la pause : la fin de la dernière entrée. */
      since: number;
      /** Secondes de la dernière entrée, figées par la pause. */
      duration: number;
      launched_by: Source | null;
      workdir: string | null;
    };

/** Une entrée avec son lot, pour la liste du jour. `end` vide : l'entrée est ouverte. */
export interface EntryView {
  id: number;
  lot: LotView;
  start: number;
  end: number | null;
  removed: boolean;
}

export interface DayView {
  date: string;
  status: DayStatus;
  entries: EntryView[];
  start: number | null;
  end: number | null;
  pauses: number;
  total: number;
}

export interface RecentLot {
  lot: LotView;
  last_end: number;
  today_total: number;
  /** Le lot en cours ou en pause. */
  current: boolean;
}

export interface SubtaskChoices {
  /** Le poste principal. */
  main: string;
  /** Les sous-tâches communes à tous les parents. */
  base: string[];
  /** Les sous-tâches propres au parent demandé. */
  own: string[];
}

/** Tout ce qu'il faut pour dessiner le popover, en un appel. */
export interface Snapshot {
  status: Status;
  today: DayView;
  recent: RecentLot[];
  choices: SubtaskChoices;
  /** L'heure du cœur, en secondes : sert à corriger l'écart avec l'horloge du navigateur. */
  server_time: number;
  /** Le dossier personnel, pour abréger les dossiers de travail en « ~/… ». */
  home: string;
}

/** Un lancement demandé par l'interface (aussi pour `switch`). `at` : secondes. */
export interface StartRequest {
  parent_monday_id?: string | null;
  parent_label?: string | null;
  name?: string | null;
  subtask?: string | null;
  subtask_monday_id?: string | null;
  at?: number | null;
}

/** Un refus du cœur : le message est en français, le code est stable. */
export interface BridgeError {
  code: string;
  message: string;
}

/** Tout ce que l'interface sait faire avec le cœur. Réelle (Tauri) ou fausse (navigateur seul). */
export interface KairosApi {
  snapshot(): Promise<Snapshot>;
  start(request: StartRequest): Promise<void>;
  switch(request: StartRequest): Promise<void>;
  stop(): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  subtaskChoices(parent: string | null): Promise<SubtaskChoices>;
  /** Appelé quand la base change (ligne de commande, Claude, l'app elle-même). Rend de quoi se désabonner. */
  onChanged(callback: () => void): () => void;
  setPopoverHeight(pixels: number): Promise<void>;
  /** Quitte l'app, en arrêtant d'abord le lot si `stopLot`. */
  quit(stopLot: boolean): Promise<void>;
  /** Appelé quand l'utilisateur demande à quitter alors qu'un lot tourne. */
  onQuitRequested(callback: () => void): () => void;
}

export const KAIROS_API = new InjectionToken<KairosApi>('KairosApi');

/** Un rejet de commande Tauri ou du faux pont est un `BridgeError` ; le reste devient une erreur interne. */
export function toBridgeError(error: unknown): BridgeError {
  if (typeof error === 'object' && error !== null && typeof (error as BridgeError).code === 'string' && typeof (error as BridgeError).message === 'string') {
    return error as BridgeError;
  }
  return { code: 'internal', message: error instanceof Error ? error.message : String(error) };
}
