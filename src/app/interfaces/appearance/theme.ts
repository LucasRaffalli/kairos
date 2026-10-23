// Le thème porte les neutres ; l'accent (voir accent.ts) porte la couleur.
export const THEMES = ['dark', 'light', 'contrast'] as const;

export type Theme = (typeof THEMES)[number];

/** Ce que l'utilisateur choisit : « auto » suit le système et n'est jamais peint. */
export type ThemePreference = Theme | 'auto';

export function isTheme(value: string | null): value is Theme {
  return THEMES.includes(value as Theme);
}

export function isThemePreference(value: string | null): value is ThemePreference {
  return value === 'auto' || isTheme(value);
}
