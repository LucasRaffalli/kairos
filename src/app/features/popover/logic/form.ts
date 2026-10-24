import type { StartRequest } from '@app/services/kairos/kairos-api';

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
