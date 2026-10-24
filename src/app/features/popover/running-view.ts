import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import { fill } from '@app/lang/fill';
import { injectT } from '@app/lang/i18n.service';
import { KairosStore } from '@app/services/kairos/kairos.store';
import { Badge, Button, Icon } from '@app/ui';

import { formatClock, formatHm, formatTimeOfDay } from './logic/clock';
import { describeLaunch } from './logic/launch';
import { lotTitle } from './logic/view';
import { TodayList } from './today-list';

/** La clé du libellé « Lancé … » selon qui a lancé l'entrée ; `null` (à la main, ou inconnu) : « Lancé à … ». */
const LAUNCH_KEY = { 'claude-code': 'launch.claudeCode', 'claude-desktop': 'launch.claudeDesktop' } as const;

/**
 * « Minuteur lancé » (maquette 220:179) et « En pause » (pas de maquette : le minuteur figé, « En
 * pause depuis … » et Reprendre à la place de Pause). Puis la liste du jour, en lecture seule.
 */
@Component({
  selector: 'app-running-view',
  imports: [Badge, Button, Icon, TodayList],
  templateUrl: './running-view.html',
  styleUrl: './running-view.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RunningView {
  protected readonly store = inject(KairosStore);
  protected readonly t = injectT();
  protected readonly fill = fill;

  /** Le lot affiché : la vue n'existe que si un lot tourne ou est en pause. */
  protected readonly status = computed(() => {
    const status = this.store.snapshot()?.status;
    return status && status.state !== 'idle' ? status : null;
  });

  protected readonly title = computed(() => {
    const status = this.status();
    return status ? lotTitle(status.lot) : '';
  });

  protected readonly clock = computed(() => formatClock(this.store.clockSeconds()));
  /** Lu par les lecteurs d'écran à la place du chrono : ne change qu'à la minute. */
  protected readonly clockSpoken = computed(() => fill(this.t()('running.clock'), { duration: formatHm(this.store.clockSeconds()) }));

  protected readonly pausedSince = computed(() => {
    const status = this.status();
    return status?.state === 'paused' ? fill(this.t()('running.pausedSince'), { time: formatTimeOfDay(status.since) }) : null;
  });

  /** « Lancé par Claude Code à 13:34 · ~/Beeside/main ». En pause, l'entrée a commencé `duration` avant la pause. */
  protected readonly launch = computed(() => {
    const status = this.status();
    const home = this.store.snapshot()?.home ?? '';
    if (!status) {
      return null;
    }
    const startedAt = status.state === 'paused' ? status.since - status.duration : status.since;
    const parts = describeLaunch({ source: status.launched_by, startedAt, workdir: status.workdir, home, hm: formatTimeOfDay });
    const text = fill(this.t()(parts.who ? LAUNCH_KEY[parts.who] : 'launch.manual'), { time: parts.time });
    return { byClaude: parts.who !== null, text: parts.folder ? `${text} · ${parts.folder}` : text };
  });
}
