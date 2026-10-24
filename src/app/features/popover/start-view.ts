import { ChangeDetectionStrategy, Component, DestroyRef, computed, effect, inject, input, signal, viewChild } from '@angular/core';

import { injectT } from '@app/lang/i18n.service';
import type { LotView } from '@app/services/kairos/kairos-api';
import { KairosStore } from '@app/services/kairos/kairos.store';
import { Button, Chip, Field, Icon, QuickResume, RecentPill, SectionTitle } from '@app/ui';

import { formatClock } from './logic/clock';
import { errorField, type ErrorField } from './logic/errors';
import { canSubmit, subtaskChips, type FormDraft } from './logic/form';
import { lotTitle, quickResumes, recentParents } from './logic/view';

/** Attente après la dernière frappe dans « ID Monday » avant de redemander les puces du parent. */
const CHOICES_DELAY_MS = 300;

/**
 * « Lancer » (maquettes 220:81 et 223:399) et « Changer de lot » (223:304, `mode="switch"`) : le
 * même formulaire en trois blocs, le récapitulatif, le bouton et « Reprendre ». En mode « switch »,
 * un rappel du lot qui s'arrêtera, avec « Annuler » pour revenir au minuteur. Le brouillon vit dans
 * le magasin : il survit à la fermeture du popover. Les heures de fin arrivent en 2d.
 */
@Component({
  selector: 'app-start-view',
  imports: [Button, Chip, Field, Icon, QuickResume, RecentPill, SectionTitle],
  templateUrl: './start-view.html',
  styleUrl: './start-view.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class StartView {
  protected readonly store = inject(KairosStore);
  protected readonly t = injectT();
  protected readonly lotTitle = lotTitle;

  readonly mode = input<'start' | 'switch'>('start');

  private readonly subtaskName = viewChild.required<Field>('subtaskName');

  protected readonly form = this.store.form;
  protected readonly check = computed(() => canSubmit(this.form()));

  /** « + Autre » a été choisi : la sous-tâche est saisie à la main. */
  private readonly other = signal(false);
  protected readonly chips = computed(() => subtaskChips(this.store.choices()).filter(Boolean));
  /** La puce sélectionnée ; `null` : « + Autre » (saisie libre, ou texte qui ne correspond à aucune puce). */
  protected readonly selectedChip = computed(() => {
    const typed = this.form().subtask.trim();
    if (this.other()) {
      return null;
    }
    if (!typed) {
      return this.store.choices().main;
    }
    return this.chips().includes(typed) ? typed : null;
  });

  protected readonly recents = computed(() => recentParents(this.store.snapshot()?.recent ?? [], 3));
  protected readonly resumes = computed(() => quickResumes(this.store.snapshot()?.recent ?? [], 3));

  /** Le lot qui s'arrêtera au changement (mode « switch »). */
  protected readonly current = computed(() => {
    const status = this.store.snapshot()?.status;
    return this.mode() === 'switch' && status && status.state !== 'idle' ? status.lot : null;
  });
  protected readonly clock = computed(() => formatClock(this.store.clockSeconds()));

  /** « Refonte / new design  ›  Front » : où iront les heures. */
  protected readonly target = computed(() => {
    const form = this.form();
    const parent = form.parentName.trim() || form.parentId.trim();
    const subtask = form.subtask.trim() || this.store.choices().main;
    return subtask ? `${parent}  ›  ${subtask}` : parent;
  });

  constructor() {
    // Changer d'ID de parent oublie aussitôt les puces de l'ancien, puis redemande celles du
    // nouveau (les sous-tâches propres à ce parent) après une courte pause de frappe.
    let timer: ReturnType<typeof setTimeout> | undefined;
    let asked: string | null | undefined;
    effect(() => {
      const parent = this.form().parentId.trim() || null;
      clearTimeout(timer);
      if (parent !== asked) {
        this.store.resetChoices();
        timer = setTimeout(() => {
          asked = parent;
          void this.store.loadChoices(parent);
        }, CHOICES_DELAY_MS);
      }
    });
    inject(DestroyRef).onDestroy(() => clearTimeout(timer));
  }

  protected patch(change: Partial<FormDraft>): void {
    this.form.update((form) => ({ ...form, ...change }));
  }

  protected pickParent(parent: { id: string; name: string }): void {
    this.patch({ parentId: parent.id, parentName: parent.name });
  }

  protected pickChip(value: string): void {
    this.other.set(false);
    this.patch({ subtask: value });
  }

  /** « + Autre » : la saisie libre, le focus dans « Nom » de la sous-tâche. */
  protected pickOther(): void {
    this.other.set(true);
    this.patch({ subtask: '' });
    this.subtaskName().focus();
  }

  protected submit(): void {
    if (!this.check().ok || this.store.busy()) {
      return;
    }
    void (this.mode() === 'switch' ? this.store.switchLot() : this.store.start());
  }

  protected resume(lot: LotView): void {
    void this.store.launchRecent(lot);
  }

  /** Le refus du cœur à afficher sous ce champ, s'il le vise (sinon il est dans le bandeau). */
  protected fieldError(field: ErrorField): string | null {
    const error = this.store.error();
    return error && errorField(error.code, this.form()) === field ? error.message : null;
  }

  /** Sous « Début » : le refus du cœur, sinon un début mal écrit (seule règle jugée ici). */
  protected timeError(): string | null {
    const check = this.check();
    return this.fieldError('time') ?? (!check.ok && check.reason === 'bad-time' ? this.t()('start.time.badFormat') : null);
  }
}
