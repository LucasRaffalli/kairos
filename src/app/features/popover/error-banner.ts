import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { injectT } from '@app/lang/i18n.service';
import { Alert } from '@app/ui';

/**
 * Le bandeau rouge sous l'en-tête : le message du cœur, tel quel. « Réessayer » relit la base
 * quand elle est inaccessible ou trop récente (`retryable`) ; les autres refus restent affichés
 * jusqu'à l'action suivante.
 */
@Component({
  selector: 'app-error-banner',
  imports: [Alert],
  template: `<k-alert kind="error" [actionLabel]="retryable() ? t()('error.retry') : null" (action)="retry.emit()">{{ message() }}</k-alert>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ErrorBanner {
  protected readonly t = injectT();
  readonly message = input.required<string>();
  readonly retryable = input(false);
  readonly retry = output<void>();
}
