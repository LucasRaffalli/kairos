import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

/** Une pastille de tâche récente : son ID Monday et son nom. Un clic remplit le formulaire. */
@Component({
  selector: 'k-recent-pill',
  template: `
    <button type="button" class="pill" [title]="id() + ' ' + name()" (click)="chosen.emit()">
      <span class="id">{{ id() }}</span>
      <span class="name">{{ name() }}</span>
    </button>
  `,
  styleUrl: './recent-pill.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RecentPill {
  readonly id = input.required<string>();
  readonly name = input.required<string>();
  readonly chosen = output<void>();
}
