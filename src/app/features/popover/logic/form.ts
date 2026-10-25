import type { LotView, StartRequest, SubtaskChoices } from '@app/services/kairos/kairos-api';

import { secondsFromHhmm } from './clock';

/** Le brouillon du formulaire « Lancer » / « Changer de lot ». Tout est du texte, comme saisi. */
export interface FormDraft {
  parentId: string;
  parentName: string;
  /** La sous-tâche choisie (puce) ou saisie (« + Autre »). Vide : le cœur prend le poste principal. */
  subtask: string;
  subtaskId: string;
  /** « HH:MM », facultatif : vide veut dire maintenant. */
  time: string;
}

export const emptyForm = (): FormDraft => ({ parentId: '', parentName: '', subtask: '', subtaskId: '', time: '' });

export type SubmitCheck = { ok: true } | { ok: false; reason: 'need-parent-or-name' | 'bad-time' };

/**
 * Seulement ce que le formulaire peut juger sans le cœur : il faut un ID ou un nom, et un début
 * lisible. Les vraies règles (futur, trop ancien, noms valides…) restent au cœur.
 */
export function canSubmit(form: FormDraft): SubmitCheck {
  if (!form.parentId.trim() && !form.parentName.trim()) {
    return { ok: false, reason: 'need-parent-or-name' };
  }
  if (form.time.trim() && secondsFromHhmm(form.time, 0) === null) {
    return { ok: false, reason: 'bad-time' };
  }
  return { ok: true };
}

const orNull = (text: string): string | null => text.trim() || null;

/** Le brouillon, tel que le pont l'attend. Avec un ID, le nom devient son libellé. */
export function toStartRequest(form: FormDraft, nowMs: number): StartRequest {
  const parentId = orNull(form.parentId);
  const parentName = orNull(form.parentName);
  return {
    parent_monday_id: parentId,
    parent_label: parentId ? parentName : null,
    name: parentId ? null : parentName,
    subtask: orNull(form.subtask),
    subtask_monday_id: orNull(form.subtaskId),
    at: form.time.trim() ? secondsFromHhmm(form.time, nowMs) : null,
  };
}

/**
 * Le lot en cours est-il celui que `request` demandait ? Sert à savoir si un « déjà en cours » est
 * le fait d'un double clic (le lot demandé tourne) ou d'un autre lot lancé ailleurs. Une sous-tâche
 * non précisée correspond à n'importe laquelle (le cœur prend alors le poste principal).
 */
export function lotMatches(lot: LotView, request: StartRequest): boolean {
  const sameParent = request.parent_monday_id ? lot.parent_monday_id === request.parent_monday_id : lot.parent_monday_id === null && lot.name === request.name;
  return (
    sameParent && (!request.subtask || lot.subtask === request.subtask) && (!request.subtask_monday_id || lot.subtask_monday_id === request.subtask_monday_id)
  );
}

/** Les puces de sous-tâche : le poste principal d'abord, puis le socle et celles du parent, sans doublon. */
export function subtaskChips(choices: SubtaskChoices): string[] {
  return [...new Set([choices.main, ...choices.base, ...choices.own])];
}

/** Relancer un lot connu (« Reprendre ») : son parent, ou son nom s'il n'a pas d'ID, et sa sous-tâche, maintenant. */
export function lotRequest(lot: LotView): StartRequest {
  const parentId = lot.parent_monday_id;
  return {
    parent_monday_id: parentId,
    parent_label: parentId ? lot.parent_label : null,
    name: parentId ? null : lot.name,
    subtask: lot.subtask,
    subtask_monday_id: lot.subtask_monday_id,
    at: null,
  };
}
