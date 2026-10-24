import type { FormDraft } from './form';

/** Les champs du formulaire sous lesquels un refus du cœur peut s'afficher. */
export type ErrorField = 'parent-name' | 'parent-id' | 'subtask-name' | 'subtask-id' | 'time';

/**
 * Les règles de forme du cœur (`clean_monday_id`, `clean_text`), reprises ici UNIQUEMENT pour
 * savoir sous quel champ placer son refus : l'interface ne refuse jamais rien avec elles.
 */
const MAX_ID_DIGITS = 20;
const MAX_NAME_CHARS = 120;

const badId = (text: string): boolean => {
  const id = text.trim();
  return id !== '' && (id.length > MAX_ID_DIGITS || !/^\d+$/.test(id));
};

const badName = (text: string): boolean => {
  const name = text.trim();
  // Le cœur compte des caractères (pas des unités UTF-16) et refuse tout caractère de contrôle (saut de ligne…).
  return name !== '' && (/\p{Cc}/u.test(name) || [...name].length > MAX_NAME_CHARS);
};

/** Le seul des deux champs qui est fautif ; `null` si aucun ou les deux (ambigu : le bandeau). */
const onlyOne = (first: boolean, second: boolean, a: ErrorField, b: ErrorField): ErrorField | null => (first === second ? null : first ? a : b);

/**
 * Sous quel champ afficher un refus du cœur, d'après le brouillon envoyé ; `null` pour le bandeau.
 * `invalid_monday_id` et `invalid_name` ne disent pas quel champ est en cause : on le déduit du
 * brouillon quand un seul des deux candidats est fautif, sinon on ne devine pas.
 */
export function errorField(code: string, form: FormDraft): ErrorField | null {
  switch (code) {
    case 'invalid_monday_id':
      return onlyOne(badId(form.parentId), badId(form.subtaskId), 'parent-id', 'subtask-id');
    case 'invalid_name':
      return onlyOne(badName(form.parentName), badName(form.subtask), 'parent-name', 'subtask-name');
    case 'same_lot':
    case 'parent_not_found':
      return form.parentId.trim() ? 'parent-id' : 'parent-name';
    case 'in_future':
    case 'too_old':
    case 'clock_behind':
    case 'invalid_time':
      return 'time';
    default:
      return null;
  }
}

/** La base est inaccessible ou trop récente : on garde la dernière vue, grisée. */
export const isStaleCode = (code: string): boolean => code === 'db_unavailable' || code === 'db_newer';
