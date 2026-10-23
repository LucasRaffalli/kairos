import { DOCUMENT, Injectable, computed, effect, inject, signal } from '@angular/core';

import { Accent, Theme, ThemePreference, isAccent, isThemePreference } from '@app/interfaces';

const THEME_KEY = 'kairos:theme';
const ACCENT_KEY = 'kairos:accent';
/**
 * Les accents cachés déjà trouvés. Une découverte se garde : sans ça, l'accent
 * secret quittait la liste dès qu'on en choisissait un autre, et il fallait
 * retaper la séquence pour y revenir.
 */
const SECRETS_KEY = 'kairos:secrets';

export interface AccentOption {
  value: Accent;
  label: string;
  /** Couleur montrée dans la pastille du sélecteur (le fond plein). */
  swatch: string;
  /** Un accent secret n'est jamais listé : voir SecretAccentService pour l'ouvrir. */
  secret?: boolean;
}

/** Liste unique des accents. Le secret est filtré tant qu'il dort. */
export const ACCENT_OPTIONS: readonly AccentOption[] = [
  { value: 'kairos', label: 'Kairos', swatch: '#8458ee' },
  { value: 'charon', label: 'Charon', swatch: '#5b7fa6' },
  { value: 'jade', label: 'Jade', swatch: '#2f9e6e' },
  { value: 'unloved', label: 'Unloved', swatch: '#d81e4a' },
  { value: 'unicorn', label: 'Unicorn', swatch: '#e0559f', secret: true },
];

@Injectable({ providedIn: 'root' })
export class ThemeService {
  private readonly document = inject(DOCUMENT);

  /** Les accents cachés déjà trouvés, gardés d'une session à l'autre. */
  private readonly _found = signal<readonly Accent[]>(readFound());
  /** Ce que l'utilisateur a choisi, `auto` compris. */
  private readonly _preference = signal<ThemePreference>('auto');
  /** Ce que le système demande, suivi en direct. */
  private readonly _system = signal<Theme>('dark');
  private readonly _accent = signal<Accent>('kairos');

  readonly preference = this._preference.asReadonly();
  readonly accent = this._accent.asReadonly();

  /**
   * Le thème RÉELLEMENT peint. C'est lui qui va dans `data-theme`, jamais la
   * préférence : les accents s'accrochent à `[data-theme='dark']` et compagnie,
   * et un attribut valant « auto » ne matcherait aucune de ces règles.
   */
  readonly theme = computed<Theme>(() => {
    const chosen = this._preference();
    return chosen === 'auto' ? this._system() : chosen;
  });

  /** Les accents proposés : un accent secret n'y entre que trouvé ou actif. */
  readonly visibleAccents = computed(() => ACCENT_OPTIONS.filter((option) => this.isAvailable(option.value)));

  constructor() {
    this.watchSystem();
    this.restore();

    effect(() => {
      // L'attribut reçoit le thème peint, le stockage la PRÉFÉRENCE : au
      // prochain lancement, « automatique » doit rester automatique.
      this.document.documentElement.dataset['theme'] = this.theme();
      localStorage.setItem(THEME_KEY, this._preference());
    });

    effect(() => {
      const accent = this._accent();
      this.document.documentElement.dataset['accent'] = accent;
      localStorage.setItem(ACCENT_KEY, accent);
    });
  }

  select(theme: ThemePreference): void {
    this._preference.set(theme);
  }

  /** Depuis l'interface : on ne choisit que ce que la grille propose. */
  selectAccent(accent: Accent): void {
    if (this.isAvailable(accent)) {
      this._accent.set(accent);
    }
  }

  /** Seule porte d'entrée de l'accent caché : le code tapé dans l'app. */
  activateSecretAccent(accent: Accent = 'unicorn'): void {
    this._accent.set(accent);
    this.remember(accent);
  }

  /**
   * Un accent qu'on a le droit de choisir : les ordinaires, ceux dont la
   * séquence a été trouvée, et l'actif (pour pouvoir en sortir). C'est le MÊME
   * prédicat qui décide de l'afficher et de le poser.
   */
  private isAvailable(accent: Accent): boolean {
    const option = ACCENT_OPTIONS.find((candidate) => candidate.value === accent);
    if (!option) {
      return false;
    }
    return !option.secret || this._found().includes(accent) || this._accent() === accent;
  }

  /** `matchMedia` et non une lecture au démarrage : macOS bascule au coucher du soleil. */
  private watchSystem(): void {
    const query = this.document.defaultView?.matchMedia('(prefers-color-scheme: light)');
    if (!query) {
      return;
    }
    const read = (): void => this._system.set(query.matches ? 'light' : 'dark');
    read();
    query.addEventListener('change', read);
  }

  private remember(accent: Accent): void {
    if (this._found().includes(accent)) {
      return;
    }
    const found = [...this._found(), accent];
    this._found.set(found);
    localStorage.setItem(SECRETS_KEY, JSON.stringify(found));
  }

  private restore(): void {
    const storedTheme = localStorage.getItem(THEME_KEY);
    const storedAccent = localStorage.getItem(ACCENT_KEY);
    // Rien d'enregistré ou valeur inconnue : on suit le système.
    this._preference.set(isThemePreference(storedTheme) ? storedTheme : 'auto');
    if (isAccent(storedAccent)) {
      this._accent.set(storedAccent);
    }
    this._found.set(readFound());
  }
}

/** Tolérant par construction : un contenu illisible ne doit jamais empêcher le démarrage. */
function readFound(): readonly Accent[] {
  try {
    const raw = JSON.parse(localStorage.getItem(SECRETS_KEY) ?? '[]');
    return Array.isArray(raw) ? raw.filter((value): value is Accent => isAccent(value)) : [];
  } catch {
    return [];
  }
}
