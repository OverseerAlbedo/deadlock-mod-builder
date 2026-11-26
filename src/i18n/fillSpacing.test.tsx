import { act, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { applyCatalog, fill, MIRROR_KEY, setLanguage, t } from './index';
import { T } from './T';

afterEach(async () => {
  await act(() => setLanguage('en'));
  try { localStorage.removeItem(MIRROR_KEY); } catch { /* none */ }
});

describe('Spaces around a value put into Chinese text', () => {
  it('drops the space on both sides when the template and the value meet in Chinese characters', () => {
    expect(fill('适配到 {hero} 的骨架上', {hero: '盖斯特夫人'})).toBe('适配到盖斯特夫人的骨架上');
    expect(fill('游戏中为 {item}', {item: '扩容弹匣'})).toBe('游戏中为扩容弹匣');
    expect(fill('{hero} 的枪械是 {hero} 模型的一部分', {hero: '盖斯特夫人'})).toBe('盖斯特夫人的枪械是盖斯特夫人模型的一部分');
  });
  it('keeps the spaces around a Latin or numeric value', () => {
    expect(fill('适配到 {hero} 的骨架上', {hero: 'Lady Geist'})).toBe('适配到 Lady Geist 的骨架上');
    expect(fill('已更改 {count} 项', {count: 3})).toBe('已更改 3 项');
  });
  it('never touches an English template', () => {
    expect(fill('Fitted to {hero} skeleton', {hero: '盖斯特夫人'})).toBe('Fitted to 盖斯特夫人 skeleton');
    expect(fill('Fitted to {hero} skeleton', {hero: 'Lady Geist'})).toBe('Fitted to Lady Geist skeleton');
  });
  it('closes up at the start and at the end of the text', () => {
    expect(fill('{hero} 的模型', {hero: '岚梦'})).toBe('岚梦的模型');
    expect(fill('改为给 {hero}', {hero: '岚梦'})).toBe('改为给岚梦');
    expect(fill('{hero}', {hero: '岚梦'})).toBe('岚梦');
  });
  it('treats two values in a row and a value with a Chinese edge on one side only', () => {
    expect(fill('由 {a} {b} 使用', {a: '岚梦', b: '海妖'})).toBe('由岚梦 海妖使用');
    expect(fill('由 {a} 和 {b} 使用', {a: '岚梦', b: 'Mo & Krill'})).toBe('由岚梦和 Mo & Krill 使用');
    expect(fill('改为 {name} 的颜色', {name: '岚梦 v2'})).toBe('改为岚梦 v2 的颜色');
    expect(fill('改为 {name} 的颜色', {name: 'v2 岚梦'})).toBe('改为 v2 岚梦的颜色');
  });
  it('treats full-width punctuation as Chinese and leaves an unknown placeholder as written', () => {
    expect(fill('{part} ，{state}', {part: '头发', state: '已更改 1 项'})).toBe('头发，已更改 1 项');
    expect(fill('“{page}” 中', {page: '设置'})).toBe('“设置” 中');
    expect(fill('（ {name} ）', {name: '岚梦'})).toBe('（岚梦）');
    expect(fill('到 {other} 的', {hero: '岚梦'})).toBe('到 {other} 的');
  });
  it('applies through a catalog entry that keeps the spaces of the English', () => {
    applyCatalog('zh-CN', {'Recolor the game\'s own {hero} instead': '改为给游戏原版的 {hero} 改色'});
    expect(t('Recolor the game\'s own {hero} instead', {hero: '岚梦'})).toBe('改为给游戏原版的岚梦改色');
    expect(t('Recolor the game\'s own {hero} instead', {hero: 'Haze'})).toBe('改为给游戏原版的 Haze 改色');
  });
  it('closes up in <T> for a text value and leaves an element value alone', () => {
    applyCatalog('zh-CN', {'Fitted to <strong>{hero}</strong> skeleton': '适配到 <strong>{hero}</strong> 的骨架上', 'Fitted to {hero} skeleton': '适配到 {hero} 的骨架上'});
    const text = render(<p data-testid="a"><T text="Fitted to {hero} skeleton" values={{hero: '盖斯特夫人'}}/></p>);
    expect(text.getByTestId('a').textContent).toBe('适配到盖斯特夫人的骨架上');
    const latin = render(<p data-testid="b"><T text="Fitted to {hero} skeleton" values={{hero: 'Lady Geist'}}/></p>);
    expect(latin.getByTestId('b').textContent).toBe('适配到 Lady Geist 的骨架上');
    const number = render(<p data-testid="c"><T text="Fitted to {hero} skeleton" values={{hero: 7}}/></p>);
    expect(number.getByTestId('c').textContent).toBe('适配到 7 的骨架上');
    const element = render(<p data-testid="d"><T text="Fitted to {hero} skeleton" values={{hero: <b>盖斯特夫人</b>}}/></p>);
    expect(element.getByTestId('d').textContent).toBe('适配到 盖斯特夫人 的骨架上');
  });
});
