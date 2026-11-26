/** A sentence with markup, translated as one piece:
 * `<T text="Source code: <code>{url}</code>." tags={{code: chunk => <code>{chunk}</code>}} values={{url}}/>`.
 * Tags are read in the (translated) text first and values inserted after, so a
 * value such as `C:\Users\<you>` is never read as markup. A tag the translation
 * names but the code does not give shows its text plainly. `context` works as in `tc`. */
import { Fragment, type ReactNode } from 'react';
import { closedGaps, contextKey, lookup, useLanguage } from './index';

export type Tags = Record<string, (chunk: ReactNode) => ReactNode>;
/** Tags every `<T>` knows without being given them: `<strong>`, `<code>`, `<em>`. */
export const BASIC_TAGS: Tags = { strong: chunk => <strong>{chunk}</strong>, code: chunk => <code>{chunk}</code>, em: chunk => <em>{chunk}</em> };
const TAG = /<(\w+)>([\s\S]*?)<\/\1>/g;

/** Inserts `values`; a string or number value closes up a space to Chinese text as `fill` does
 * (`closedGaps`); an element value is left alone. */
function withValues(text: string, values: Record<string, ReactNode> | undefined): ReactNode[] {
  if (!values) return text ? [text] : [];
  const out: ReactNode[] = [];
  let last = 0;
  let skip = false;
  for (const match of text.matchAll(/\{(\w+)\}/g)) {
    const start = match.index;
    const end = start + match[0].length;
    const name = match[1];
    let piece = text.slice(last + (skip ? 1 : 0), start);
    skip = false;
    if (!(name in values)) {
      out.push(piece, match[0]);
    } else {
      const value = values[name];
      if (typeof value === 'string' || typeof value === 'number') {
        const [before, after] = closedGaps(text, start, end, String(value));
        if (before && piece.endsWith(' ')) piece = piece.slice(0, -1);
        skip = after;
      }
      out.push(piece, value);
    }
    last = end;
  }
  out.push(text.slice(last + (skip ? 1 : 0)));
  return out.filter(part => part !== '');
}
function build(text: string, values: Record<string, ReactNode> | undefined, tags: Tags | undefined): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(TAG)) {
    out.push(...withValues(text.slice(last, match.index), values));
    const inner = build(match[2], values, tags);
    const tag = tags?.[match[1]];
    out.push(tag ? tag(<>{inner.map((piece, i) => <Fragment key={i}>{piece}</Fragment>)}</>) : inner);
    last = match.index + match[0].length;
  }
  out.push(...withValues(text.slice(last), values));
  return out;
}
/** Renders the translation of `text` with its markup. */
export function T({ text, values, tags, context }: { text: string; values?: Record<string, ReactNode>; tags?: Tags; context?: string }) {
  useLanguage();
  const shown = lookup(context === undefined ? text : contextKey(context, text), text);
  return <>{build(shown, values, { ...BASIC_TAGS, ...tags }).flat().map((piece, i) => <Fragment key={i}>{piece}</Fragment>)}</>;
}
