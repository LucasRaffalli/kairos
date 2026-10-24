import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import { fill } from '@app/lang/fill';
import { injectT } from '@app/lang/i18n.service';
import type { LotView } from '@app/services/kairos/kairos-api';
import { KairosStore } from '@app/services/kairos/kairos.store';
import { SectionTitle } from '@app/ui';

import { formatHm, formatTimeOfDay } from './logic/clock';
import { todayRows, type TodayRow } from './logic/today';
import { lotTitle, parentColorIndex } from './logic/view';

/**
 * « AUJOURD'HUI · 3 h 54 » et les entrées du jour, en lecture seule (modifier arrive en 2b).
 * L'entrée en cours est surlignée, sa durée en vert ET suivie de « → … » et d'un texte lu par
 * les lecteurs d'écran : la couleur ne porte pas seule l'état.
 */
@Component({
  selector: 'app-today-list',
  imports: [SectionTitle],
  template: `
    @if (day().rows.length) {
      <k-section-title [title]="fill(t()('today.title'), { total: formatHm(day().total) })" />
      <ul class="rows">
        @for (row of day().rows; track row.id) {
          <li class="row" [class.open]="row.open">
            <span class="swatch" [attr.data-color]="color(row.lot)" aria-hidden="true"></span>
            <span class="text">
              <span class="name" [title]="lotTitle(row.lot)">{{ lotTitle(row.lot) }}</span>
              <span class="meta" [title]="meta(row)">{{ meta(row) }}</span>
            </span>
            <span class="duration">
              {{ formatHm(row.seconds) }}
              @if (row.open) {
                <span class="sr-only">{{ t()('today.open') }}</span>
              }
            </span>
          </li>
        }
      </ul>
    }
  `,
  styleUrl: './today-list.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class TodayList {
  private readonly store = inject(KairosStore);
  protected readonly t = injectT();
  protected readonly fill = fill;
  protected readonly formatHm = formatHm;
  protected readonly lotTitle = lotTitle;

  protected readonly day = computed(() => {
    const today = this.store.snapshot()?.today;
    return today ? todayRows(today, this.store.clockSeconds()) : { rows: [], total: 0 };
  });

  /** La couleur du parent (ID, à défaut le nom du lot), de 0 à 5. */
  protected color(lot: LotView): number {
    return parentColorIndex(lot.parent_monday_id ?? lot.name ?? '');
  }

  /** « 12406850311 · Front · 13:34 → … », « sans ID · 10:13 → 11:31 ». */
  protected meta(row: TodayRow): string {
    const span = `${formatTimeOfDay(row.start)} → ${row.end === null ? '…' : formatTimeOfDay(row.end)}`;
    return [row.lot.parent_monday_id ?? this.t()('today.noId'), row.lot.subtask, span].filter(Boolean).join(' · ');
  }
}
