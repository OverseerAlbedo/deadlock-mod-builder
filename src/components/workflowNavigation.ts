/** Reveal an existing control without altering project data or native contracts. */
export function revealSection(id: string) {
  const element = window.document.getElementById(id);
  if (!element) return;
  let parent: HTMLElement | null = element;
  while (parent) { if (parent instanceof HTMLDetailsElement) parent.open = true; parent = parent.parentElement; }
  element.querySelectorAll<HTMLDetailsElement>('details[data-open-on-navigate]').forEach(details => { details.open = true; });
  element.scrollIntoView?.({ block: 'start', behavior: window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
  const control = element.querySelector<HTMLElement>('input:not(:disabled), select:not(:disabled), button:not(:disabled), summary');
  if (id === 'character-selection' && control instanceof HTMLButtonElement && control.getAttribute('aria-expanded') !== 'true') control.click();
  else control?.focus({ preventScroll: true });
}
