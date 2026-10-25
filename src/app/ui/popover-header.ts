import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/** L'en-tête du popover : le glyphe Kairos, le titre et sa ligne de contexte ; les boutons projetés sont rangés à droite. */
@Component({
  selector: 'k-popover-header',
  template: `
    <span class="mark" aria-hidden="true"></span>
    <div class="who">
      <strong class="title">{{ title() }}</strong>
      @if (subtitle(); as text) {
        <span class="subtitle">{{ text }}</span>
      }
    </div>
    <div class="actions"><ng-content /></div>
  `,
  styleUrl: './popover-header.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PopoverHeader {
  readonly title = input.required<string>();
  readonly subtitle = input<string | null>(null);
}
