/** Reveal an existing control without altering project data or native contracts. */
export function revealSection(id: string) {
  const element = window.document.getElementById(id);
  if (!element) return;
  let parent: HTMLElement | null = element;
  while (parent) {
    if (parent instanceof HTMLDetailsElement) parent.open = true;
    // A target in a hidden tab panel (a studio page's tab): open its tab first.
    const panel = parent.id;
    if (parent.getAttribute('role') === 'tabpanel' && parent.hidden && panel) [...window.document.querySelectorAll<HTMLElement>('[role="tab"]')].find(tab => tab.getAttribute('aria-controls') === panel)?.click();
    parent = parent.parentElement;
  }
  // A closed top-bar menu (its panel is rendered only when open): the target wraps its button.
  if (element.hasAttribute('data-reveal-opens')) {
    const button = element.querySelector<HTMLButtonElement>('button[aria-expanded="false"]');
    if (button && !button.disabled) button.click();
  }
  element.querySelectorAll<HTMLDetailsElement>('details[data-open-on-navigate]').forEach(details => { details.open = true; });
  element.scrollIntoView?.({ block: 'start', behavior: window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
  const control = element.querySelector<HTMLElement>('input:not(:disabled), select:not(:disabled), button:not(:disabled), summary');
  if (id === 'character-selection' && control instanceof HTMLButtonElement && control.getAttribute('aria-expanded') !== 'true') control.click();
  else control?.focus({ preventScroll: true });
}
