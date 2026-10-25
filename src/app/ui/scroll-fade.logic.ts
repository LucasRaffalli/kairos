/** La position d'une zone qui défile, sur un axe : `scrollTop`/`scrollLeft`, `scrollHeight`/`scrollWidth`, `clientHeight`/`clientWidth`. */
export interface ScrollMetrics {
  offset: number;
  size: number;
  client: number;
}

/** Reste-t-il du contenu à voir avant (haut, gauche) et après (bas, droite) ? */
export interface MoreContent {
  before: boolean;
  after: boolean;
}

/** Les positions sont fractionnaires sur les écrans Retina : un écart d'un pixel ne compte pas. */
export const SCROLL_TOLERANCE = 1;

export function moreContent({ offset, size, client }: ScrollMetrics): MoreContent {
  if (size - client <= SCROLL_TOLERANCE) {
    return { before: false, after: false };
  }
  return {
    before: offset > SCROLL_TOLERANCE,
    after: offset + client < size - SCROLL_TOLERANCE,
  };
}
