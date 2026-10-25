import { ChangeDetectionStrategy, Component, input } from '@angular/core';

export type IconName = 'play' | 'stop' | 'pause' | 'swap' | 'alert' | 'info' | 'term';

/**
 * Un glyphe au trait, à la couleur du texte autour (`currentColor`), de la taille de la police (1em).
 * Décoratif : le sens passe par le texte. Les tracés sont ceux de la maquette Figma (dessinés à la
 * main, dans le style Feather/Lucide ; ce n'est pas la bibliothèque Lucide).
 */
@Component({
  selector: 'k-icon',
  template: `
    @switch (name()) {
      <!-- Les quatre glyphes des boutons reprennent la géométrie de la maquette (nœud 245:348) : chacun sa grille et son épaisseur de trait. -->
      @case ('play') {
        <svg viewBox="0 0 15 15" stroke-width="1.25" aria-hidden="true" focusable="false"><path d="M5 3.4375 11.875 7.5 5 11.5625z" /></svg>
      }
      @case ('stop') {
        <svg viewBox="0 0 15 15" stroke-width="1.25" aria-hidden="true" focusable="false">
          <path d="M5 3.75h5c.69 0 1.25.56 1.25 1.25v5c0 .69-.56 1.25-1.25 1.25H5c-.69 0-1.25-.56-1.25-1.25V5c0-.69.56-1.25 1.25-1.25z" />
        </svg>
      }
      @case ('pause') {
        <svg viewBox="0 0 14 14" stroke-width="1.05" aria-hidden="true" focusable="false">
          <path
            d="M4.08 2.92h1.17c.32 0 .58.26.58.58v7c0 .32-.26.58-.58.58H4.08a.58.58 0 0 1-.58-.58v-7c0-.32.26-.58.58-.58zM8.75 2.92h1.17c.32 0 .58.26.58.58v7c0 .32-.26.58-.58.58H8.75a.58.58 0 0 1-.58-.58v-7c0-.32.26-.58.58-.58z"
          />
        </svg>
      }
      @case ('swap') {
        <svg viewBox="0 0 16 16" stroke-width="1.2" aria-hidden="true" focusable="false">
          <path d="M4.67 8 2 5.33l2.67-2.66M2 5.33h9.33M11.33 8 14 10.67l-2.67 2.66M14 10.67H4.67" />
        </svg>
      }
      @default {
        <svg viewBox="0 0 16 16" stroke-width="1.5" aria-hidden="true" focusable="false">
          @switch (name()) {
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
      }
    }
  `,
  styleUrl: './icon.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Icon {
  readonly name = input.required<IconName>();
}
