import { ChangeDetectionStrategy, Component, DestroyRef, ElementRef, afterNextRender, computed, inject, viewChild } from '@angular/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { fill } from '@app/lang/fill';
import { I18nService, injectT } from '@app/lang/i18n.service';
import { KAIROS_API } from '@app/services/kairos/kairos-api';
import { KairosStore } from '@app/services/kairos/kairos.store';
import { PopoverHeader } from '@app/ui';

import { ErrorBanner } from './error-banner';
import { errorField } from './logic/errors';
import { lotTitle, quitSummary } from './logic/view';
import { QuitDialog } from './quit-dialog';
import { RunningView } from './running-view';
import { StartView } from './start-view';

/**
 * Le popover de la barre de menus : l'en-tête, le bandeau d'erreur et la vue choisie par l'état du
 * cœur (`pickView`). Seule la coquille ferme la fenêtre, et seulement sur Échap : aucune action ne
 * la ferme (perdre le focus la cache, côté Rust). Elle porte les raccourcis ⌘↵ et ⌘., ajuste la
 * hauteur de la fenêtre au contenu et montre le dialogue « Quitter » quand le magasin l'ouvre.
 */
@Component({
  selector: 'app-popover',
  imports: [PopoverHeader, ErrorBanner, StartView, RunningView, QuitDialog],
  templateUrl: './popover.html',
  styleUrl: './popover.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(document:keydown)': 'onKey($event)' },
})
export class Popover {
  private readonly i18n = inject(I18nService);
  private readonly api = inject(KAIROS_API);
  protected readonly store = inject(KairosStore);
  protected readonly t = injectT();

  private readonly frame = viewChild.required<ElementRef<HTMLElement>>('frame');

  /** « lun. 20 oct. » : la date du jour, dans la langue de l'interface. */
  private readonly today = computed(() =>
    new Intl.DateTimeFormat(this.i18n.language(), { weekday: 'short', day: 'numeric', month: 'short' }).format(new Date()),
  );

  /** « lun. 20 oct. · 1 lot en cours ». */
  protected readonly subtitle = computed(() => {
    const state = this.store.snapshot()?.status.state ?? 'idle';
    const key = state === 'running' ? 'popover.running' : state === 'paused' ? 'popover.paused' : 'popover.idle';
    return `${this.today()} · ${this.t()(key)}`;
  });

  /**
   * Le refus à montrer dans le bandeau : ceux qui ne visent aucun champ, et tous ceux qui
   * surviennent sur l'écran du minuteur (il n'a pas de champ où les montrer).
   */
  protected readonly bannerError = computed(() => {
    const error = this.store.error();
    if (!error) {
      return null;
    }
    const view = this.store.view();
    return view === 'running' || view === 'paused' || errorField(error.code, this.store.form()) === null ? error : null;
  });

  /** Annoncé par les lecteurs d'écran au changement d'état (pas à chaque seconde du chrono). */
  protected readonly liveText = computed(() => {
    const status = this.store.snapshot()?.status;
    const t = this.t();
    if (this.store.view() === 'switch') {
      return t('popover.live.switch');
    }
    if (status?.state === 'running') {
      return fill(t('popover.live.running'), { lot: lotTitle(status.lot) });
    }
    if (status?.state === 'paused') {
      return fill(t('popover.live.paused'), { lot: lotTitle(status.lot) });
    }
    return t('popover.live.idle');
  });

  /** « Front · 12406850311 · 0 h 42 » : le lot que « Arrêter le lot et quitter » arrêterait. */
  protected readonly quitSummary = computed(() => {
    const status = this.store.snapshot()?.status;
    return status ? quitSummary(status, this.store.clockSeconds()) : null;
  });

  constructor() {
    // Le faux pont ne sait pas ouvrir « Changer de lot » : le scénario du navigateur le demande ici.
    if (new URLSearchParams(window.location.search).get('scenario') === 'switching') {
      this.store.openSwitch();
    }

    // La fenêtre prend la hauteur du contenu (Rust la plafonne à l'écran ; au-delà, on défile).
    let observer: ResizeObserver | null = null;
    afterNextRender(() => {
      const frame = this.frame().nativeElement;
      observer = new ResizeObserver(() => void this.api.setPopoverHeight(Math.ceil(frame.getBoundingClientRect().height)));
      observer.observe(frame);
    });

    inject(DestroyRef).onDestroy(() => observer?.disconnect());
  }

  protected onKey(event: KeyboardEvent): void {
    if (event.isComposing) {
      return;
    }
    if (event.key === 'Escape') {
      event.preventDefault();
      if (this.store.quitting()) {
        this.store.cancelQuit();
      } else if ('__TAURI_INTERNALS__' in window) {
        // Côté Rust, une demande de fermeture du popover le cache sans le détruire.
        void getCurrentWindow().close();
      }
      return;
    }
    if (!event.metaKey || this.store.quitting() || this.store.stale()) {
      return;
    }
    const view = this.store.view();
    if (event.key === 'Enter' && (view === 'start' || view === 'switch')) {
      event.preventDefault();
      void (view === 'start' ? this.store.start() : this.store.switchLot());
    } else if (event.key === '.' && view !== 'start') {
      event.preventDefault();
      void this.store.stop();
    }
  }
}
