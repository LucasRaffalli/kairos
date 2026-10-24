import { ChangeDetectionStrategy, Component, input } from '@angular/core';

export type IconName = 'play' | 'stop' | 'pause' | 'swap' | 'alert' | 'info' | 'term';

/** Un glyphe de 16 px au trait, à la couleur du texte autour (`currentColor`). Décoratif : le sens passe par le texte. */
@Component({
  selector: 'k-icon',
  template: `
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      @switch (name()) {
        @case ('play') {
          <path d="M5 3.5v9l7.5-4.5z" />
        }
        @case ('stop') {
          <rect x="4" y="4" width="8" height="8" rx="1.5" />
        }
        @case ('pause') {
          <path d="M5.5 3.5v9M10.5 3.5v9" />
        }
        @case ('swap') {
          <path d="M3 5.5h10M10.5 3l2.5 2.5-2.5 2.5M13 10.5H3M5.5 8 3 10.5 5.5 13" />
        }
        @case ('alert') {
          <path d="M8 2.5 14 13H2zM8 6.5v3M8 11.5v.01" />
        }
        @case ('term') {
          <path d="M3 4.5 6.5 8 3 11.5M8.5 11.5H13" />
        }
        @case ('info') {
          <circle cx="8" cy="8" r="5.5" />
          <path d="M8 7.5v3.5M8 5v.01" />
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
