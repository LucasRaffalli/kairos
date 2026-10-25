import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Icon, IconName } from './icon';

export type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost';

/**
 * Le bouton du popover. Le libellé est le contenu projeté ; un bouton sans texte (icône seule)
 * doit recevoir `ariaLabel`. `kbd` affiche à droite la puce du raccourci clavier.
 */
@Component({
  selector: 'k-button',
  imports: [Icon],
  template: `
    <button type="button" [class]="'btn ' + variant()" [disabled]="disabled()" [attr.aria-label]="ariaLabel()" (click)="pressed.emit()">
      @if (icon(); as name) {
        <k-icon [name]="name" [attr.data-icon]="name" />
      }
      <span class="label"><ng-content /></span>
      @if (kbd(); as keys) {
        <kbd aria-hidden="true">{{ keys }}</kbd>
      }
    </button>
  `,
  styleUrl: './button.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Button {
  readonly variant = input<ButtonVariant>('primary');
  readonly disabled = input(false);
  /** Le raccourci affiché à droite, par exemple « ↵ » ou « ⌘S ». */
  readonly kbd = input<string | null>(null);
  readonly icon = input<IconName | null>(null);
  /** Le nom accessible d'un bouton sans texte. */
  readonly ariaLabel = input<string | null>(null);
  readonly pressed = output<void>();
}
