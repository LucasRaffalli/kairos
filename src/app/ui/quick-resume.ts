import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Icon } from './icon';

/** La reprise rapide d'un lot récent : un clic le relance sur-le-champ. L'ID et la sous-tâche sont facultatifs (lot sans ID). */
@Component({
  selector: 'k-quick-resume',
  imports: [Icon],
  template: `
    <button type="button" class="resume" [title]="name() + (details() ? ' · ' + details() : '')" (click)="resumed.emit()">
      <k-icon class="play filled" name="play" />
      <span class="name">{{ name() }}</span>
      @if (details(); as text) {
        <span class="details">{{ text }}</span>
      }
    </button>
  `,
  styleUrl: './quick-resume.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class QuickResume {
  readonly name = input.required<string>();
  readonly id = input<string | null>(null);
  readonly subtask = input<string | null>(null);
  readonly resumed = output<void>();

  /** « 12406850311 · Front », sans les morceaux absents. */
  protected readonly details = computed(() => [this.id(), this.subtask()].filter(Boolean).join(' · '));
}
