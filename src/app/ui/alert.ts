import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Icon } from './icon';

export type AlertKind = 'error' | 'info';

/**
 * Le bandeau d'une erreur ou d'une information ; le message est le contenu projeté. Un glyphe
 * accompagne la couleur. Avec `actionLabel`, un bouton (« Réessayer ») émet `action`.
 */
@Component({
  selector: 'k-alert',
  imports: [Icon],
  template: `
    <div class="alert" [class]="kind()" [attr.role]="kind() === 'error' ? 'alert' : 'status'">
      <k-icon class="glyph" [name]="kind() === 'error' ? 'alert' : 'info'" />
      <p class="message"><ng-content /></p>
      @if (actionLabel(); as label) {
        <button type="button" class="action" (click)="action.emit()">{{ label }}</button>
      }
    </div>
  `,
  styleUrl: './alert.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Alert {
  readonly kind = input<AlertKind>('error');
  readonly actionLabel = input<string | null>(null);
  readonly action = output<void>();
}
