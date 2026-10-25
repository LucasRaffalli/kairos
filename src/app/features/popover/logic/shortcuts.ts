/** Ce qu'il faut lire d'un événement clavier pour reconnaître un raccourci. */
export interface KeyChord {
  key: string;
  code: string;
  metaKey: boolean;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
}

/**
 * ⌘S (« Stopper ») arrête le lot, sur tous les claviers : c'est le raccourci affiché sur le bouton.
 * ⌘. l'arrête aussi. Sur un clavier AZERTY, le point demande Maj : la touche est celle de la virgule
 * du clavier américain (`code` « Comma »), et avec ⌘ le navigateur peut rapporter « . » ou, sans
 * tenir compte de Maj, « ; ». On reconnaît donc « . » tout court, ou Maj + la touche `Comma` quand
 * la valeur rapportée est « . » ou « ; ». Sur un clavier américain, ⌘, et ⌘⇧, (« < ») restent libres.
 */
export function isStopShortcut(event: KeyChord): boolean {
  if (!event.metaKey || event.ctrlKey || event.altKey) {
    return false;
  }
  if (event.key.toLowerCase() === 's' && !event.shiftKey) {
    return true;
  }
  if (event.key === '.') {
    return true;
  }
  return event.shiftKey && event.code === 'Comma' && event.key === ';';
}
