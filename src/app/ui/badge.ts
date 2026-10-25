import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/** Une étiquette en capitales, par exemple le nom de la sous-tâche d'un lot. */
@Component({
  selector: 'k-badge',
  template: `<span class="badge">{{ text() }}</span>`,
  styleUrl: './badge.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Badge {
  readonly text = input.required<string>();
}
