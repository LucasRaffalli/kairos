import { Injectable, computed, inject, signal } from '@angular/core';

import { en } from './en';
import { fr } from './fr';

export type TranslationKey = keyof typeof fr;
export type Language = 'fr' | 'en';

const LANGUAGE_KEY = 'kairos:lang';
const DICTIONARIES: Record<Language, Record<TranslationKey, string>> = { fr, en };

/** La langue de l'interface : celle du système au premier lancement, puis le choix retenu. */
@Injectable({ providedIn: 'root' })
export class I18nService {
  private readonly _language = signal<Language>(initialLanguage());
  readonly language = this._language.asReadonly();

  /** Le dictionnaire courant, relu à chaque changement de langue. */
  readonly t = computed(() => {
    const dictionary = DICTIONARIES[this._language()];
    return (key: TranslationKey): string => dictionary[key];
  });

  set(language: Language): void {
    this._language.set(language);
    localStorage.setItem(LANGUAGE_KEY, language);
  }
}

/** Raccourci pour un composant : `t = injectT()` puis `t()('popover.title')`. */
export function injectT() {
  return inject(I18nService).t;
}

function initialLanguage(): Language {
  const stored = localStorage.getItem(LANGUAGE_KEY);
  if (stored === 'fr' || stored === 'en') {
    return stored;
  }
  return navigator.language.toLowerCase().startsWith('fr') ? 'fr' : 'en';
}
