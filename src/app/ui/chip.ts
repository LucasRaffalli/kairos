import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

/** Une puce de choix (sous-tâche). `dashed` : la puce « + Autre » qui ouvre la saisie libre. */
@Component({
  selector: 'k-chip',
  template: `
    <button type="button" class="chip" [class.dashed]="dashed()" [attr.aria-pressed]="selected()" [title]="value()" (click)="chosen.emit(value())">
      <span class="text">{{ value() }}</span>
    </button>
  `,
  styleUrl: './chip.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Chip {
  readonly value = input.required<string>();
  readonly selected = input(false);
  readonly dashed = input(false);
  /** Émet `value` quand on choisit la puce. */
  readonly chosen = output<string>();
}
