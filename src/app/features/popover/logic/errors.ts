export type ErrorField = 'parent' | 'subtask' | 'time';

/**
 * Sous quel champ afficher un refus du cœur, `null` pour le bandeau. `invalid_name` ne dit pas
 * quel nom est en cause : on suppose le parent, sauf si seule la sous-tâche est personnalisée.
 */
export function errorField(code: string, customSubtask = false): ErrorField | null {
  switch (code) {
    case 'invalid_monday_id':
    case 'same_lot':
    case 'parent_not_found':
      return 'parent';
    case 'invalid_name':
      return customSubtask ? 'subtask' : 'parent';
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
