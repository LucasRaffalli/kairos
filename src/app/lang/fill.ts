/**
 * Remplit les trous `{nom}` d'un texte traduit : `fill('Lancé à {time}', { time: '13:34' })`.
 * Un trou sans valeur reste tel quel, pour se voir à l'écran plutôt que de disparaître.
 */
export function fill(text: string, values: Record<string, string | number>): string {
  return text.replace(/\{(\w+)\}/g, (hole, name: string) => (name in values ? String(values[name]) : hole));
}
