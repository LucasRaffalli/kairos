import { ChangeDetectionStrategy, Component, input } from '@angular/core';

export type IconName = 'play' | 'stop' | 'pause' | 'swap' | 'alert' | 'info' | 'term';

/**
 * Un glyphe de la bibliothèque Lucide (https://lucide.dev, licence ISC : voir NOTICE), au trait, à la
 * couleur du texte autour (`currentColor`), de la taille de la police (1em). Les tracés sont copiés
 * tels quels (grille de 24, trait de 2) : sept icônes ne justifient pas une dépendance. Décoratif :
 * le sens passe par le texte.
 */
@Component({
  selector: 'k-icon',
  template: `
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      @switch (name()) {
        @case ('play') {
          <path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z" />
        }
        @case ('stop') {
          <rect width="18" height="18" x="3" y="3" rx="2" />
        }
        @case ('pause') {
          <rect x="14" y="3" width="5" height="18" rx="1" />
          <rect x="5" y="3" width="5" height="18" rx="1" />
        }
        @case ('swap') {
          <path d="M8 3 4 7l4 4" />
          <path d="M4 7h16" />
          <path d="m16 21 4-4-4-4" />
          <path d="M20 17H4" />
        }
        @case ('alert') {
          <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
          <path d="M12 9v4" />
          <path d="M12 17h.01" />
        }
        @case ('term') {
          <path d="M12 19h8" />
          <path d="m4 17 6-6-6-6" />
        }
        @case ('info') {
          <circle cx="12" cy="12" r="10" />
          <path d="M12 16v-4" />
          <path d="M12 8h.01" />
        }
      }
    </svg>
  `,
  styleUrl: './icon.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Icon {
  readonly name = input.required<IconName>();
}
