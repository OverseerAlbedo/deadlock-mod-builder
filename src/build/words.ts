// i18n-area: build
import { t, tc } from '../i18n';

/** Names shown as a list: "a, b, c" in English, "a、b、c" in Chinese. The names themselves stay as they come. */
export const nameList = (names: readonly string[]) => names.join(tc('list separator', ', '));
/** Counts shown in one line ("1 error, 4 successes"): ", " in English, a full-width comma in Chinese. */
export const countList = (parts: readonly string[]) => parts.join(tc('count separator', ', '));

/** Chinese script or full-width punctuation. */
const CHINESE = /[　-〿㐀-䶿一-鿿豈-﫿＀-￯]/;

/** Sentences shown one after another (empty ones left out): a space between them in English, none
 * between two Chinese ones (the first ending in Chinese, such as 。, the next holding Chinese, even
 * when it starts with a number or a name). Next to an English sentence from the backend, or after
 * one that ends in English words, they keep the space. */
export function sentences(...parts: string[]): string {
  const shown = parts.filter(Boolean);
  return shown.length ? shown.reduce((first, second) =>
    CHINESE.test(first.slice(-1)) && CHINESE.test(second) ? t('{first} {second}', { first, second }) : `${first} ${second}`) : '';
}
