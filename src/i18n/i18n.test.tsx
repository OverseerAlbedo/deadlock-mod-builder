import { act, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { applyCatalog, contextKey, getLanguage, loadCatalog, MIRROR_KEY, pluralKey, readMirror, setLanguage, t, tc, tp, tpc, useT, windowsLanguage } from './index';
import { T } from './T';
import { decideLanguage, forgetLaunch, isFirstStart, launchFacts, startLanguage, toolsSet } from './startup';

afterEach(async () => {
  await act(() => setLanguage('en'));
  try { localStorage.removeItem(MIRROR_KEY); } catch { /* none */ }
  forgetLaunch();
});

describe('The language module', () => {
  it('shows English for text the catalog lacks or leaves empty', () => {
    applyCatalog('zh-CN', {'Save settings': '保存设置', 'Detect tools': ''});
    expect(t('Save settings')).toBe('保存设置');
    expect(t('Detect tools')).toBe('Detect tools');
    expect(t('Check paths')).toBe('Check paths');
    expect(t('Use {path}', {path: 'C:\\CSDK13'})).toBe('Use C:\\CSDK13');
  });
  it('keeps a context entry apart from the plain one and falls back to the plain English', () => {
    applyCatalog('zh-CN', {'Original': '原始', [contextKey('theme name', 'Original')]: '原版'});
    expect(t('Original')).toBe('原始');
    expect(tc('theme name', 'Original')).toBe('原版');
    expect(tc('another place', 'Original')).toBe('Original');
    applyCatalog('zh-CN', {[contextKey('theme name', 'Original')]: '原版'});
    expect(t('Original')).toBe('Original');
  });
  it('keys a count apart from plain text with the same English', () => {
    const forms = {one: '{count} file', other: '{count} files'};
    expect(pluralKey(forms)).toBe('#|{count} files');
    expect(pluralKey(forms, 'list')).toBe('#list|{count} files');
    applyCatalog('zh-CN', {'{count} files': '不应使用', [pluralKey(forms)]: {other: '{count} 个文件'}, [pluralKey(forms, 'list')]: {other: '列表中 {count} 个'}});
    expect(tp(3, forms)).toBe('3 个文件');
    expect(tpc('list', 3, forms)).toBe('列表中 3 个');
    expect(tpc('other place', 1, forms)).toBe('1 file');
  });
  it('formats a count with the Russian plural forms through Intl.PluralRules', () => {
    const english = {one: "One notification is ignored in the sidebar's Notifications.", other: "{count} notifications are ignored in the sidebar's Notifications."};
    expect([1, 2, 5, 21].map(n => tp(n, english))).toEqual(["One notification is ignored in the sidebar's Notifications.", "2 notifications are ignored in the sidebar's Notifications.", "5 notifications are ignored in the sidebar's Notifications.", "21 notifications are ignored in the sidebar's Notifications."]);
    applyCatalog('ru', {[pluralKey(english)]: {one: '{count} уведомление игнорируется', few: '{count} уведомления игнорируются', many: '{count} уведомлений игнорируется', other: '{count} уведомления игнорируется'}});
    expect([1, 2, 5, 21, 1.5].map(n => tp(n, english))).toEqual(['1 уведомление игнорируется', '2 уведомления игнорируются', '5 уведомлений игнорируется', '21 уведомление игнорируется', '1.5 уведомления игнорируется']);
  });
  it('prefers an exact form and fills other values beside the count', () => {
    applyCatalog('zh-CN', {[pluralKey({other: 'Fix all {count} marked paths'})]: {'=1': '修复标记的路径', other: '修复全部 {count} 个标记的路径'}});
    const forms = {one: 'Fix the marked path', other: 'Fix all {count} marked paths'};
    expect([tp(1, forms), tp(4, forms)]).toEqual(['修复标记的路径', '修复全部 4 个标记的路径']);
    expect(tp(2, {other: '{count} in {place}'}, {place: 'C:\\x'})).toBe('2 in C:\\x');
  });
  it('reads markup from the translation and never from a value', () => {
    render(<T text="Example: <code>{example}</code>" values={{example: String.raw`C:\Users\<you>\<b>x</b>`}}/>);
    expect(document.querySelectorAll('code')).toHaveLength(1);
    expect(document.querySelector('code')?.textContent).toBe(String.raw`C:\Users\<you>\<b>x</b>`);
    expect(document.querySelector('b')).toBeNull();
    expect(document.body.textContent).toBe(String.raw`Example: C:\Users\<you>\<b>x</b>`);
  });
  it('draws the translation’s markup, with a context, and shows an unknown tag as plain text', () => {
    applyCatalog('zh-CN', {[contextKey('guide', 'Click <strong>Save</strong>.')]: '点击<strong>保存</strong>，<odd>然后</odd>继续。'});
    render(<p data-testid="line"><T context="guide" text="Click <strong>Save</strong>."/></p>);
    expect(screen.getByTestId('line').querySelector('strong')?.textContent).toBe('保存');
    expect(screen.getByTestId('line').textContent).toBe('点击保存，然后继续。');
  });
  it('switches every component that shows text at once, without a reload, and back', async () => {
    function Label() { const tr = useT(); return <span>{tr('Save settings')}</span>; }
    render(<><Label/><p><T text="Click <strong>Detect tools</strong>. It fills in what it finds and corrects common mix-ups."/></p></>);
    expect(screen.getByText('Save settings')).toBeInTheDocument();
    expect(document.querySelector('strong')?.textContent).toBe('Detect tools');
    await act(() => setLanguage('zh-CN'));
    expect(getLanguage()).toBe('zh-CN');
    expect(screen.getByText('保存设置')).toBeInTheDocument();
    expect(document.querySelector('strong')?.textContent).toBe('检测工具');
    expect(document.documentElement.lang).toBe('zh-Hans');
    expect(readMirror()).toBe('zh-CN');
    await act(() => setLanguage('en'));
    expect(screen.getByText('Save settings')).toBeInTheDocument();
    expect(document.documentElement.lang).toBe('en');
  });
  it('loads every area file of a language together, and an unknown language is English', async () => {
    const catalog = await loadCatalog('zh-CN');
    expect(catalog['Detect tools']).toBe('检测工具');           // settings.json
    expect(catalog['Make your mod']).toBe('制作模组');          // shell.json
    expect(catalog['Start with your project']).toBe('从你的项目开始'); // guide.json
    await act(() => setLanguage('xx-YY'));
    expect(getLanguage()).toBe('en');
  });
  it('lets the newest of two quick changes win', async () => {
    const first = setLanguage('zh-CN'), second = setLanguage('en');
    await act(() => Promise.all([first, second]));
    expect(getLanguage()).toBe('en');
  });
  it('reads any Chinese Windows locale as 简体中文 and everything else as English', () => {
    expect(['zh-CN', 'zh-TW', 'zh-Hans-CN', 'zh', 'ZH-hk'].map(locale => windowsLanguage(locale))).toEqual(['zh-CN', 'zh-CN', 'zh-CN', 'zh-CN', 'zh-CN']);
    expect(['en-US', 'de-DE', 'ja-JP', '', undefined].map(locale => windowsLanguage(locale))).toEqual(['en', 'en', 'en', 'en', 'en']);
  });
});

describe('The start-up language', () => {
  it('follows the saved language, a first start follows Windows, an existing user is offered Chinese', () => {
    expect(decideLanguage('zh-CN', false, 'en')).toEqual({language: 'zh-CN', persist: false, offer: false});
    expect(decideLanguage('en', true, 'zh-CN')).toEqual({language: 'en', persist: false, offer: false});
    expect(decideLanguage('xx', true, 'zh-CN')).toEqual({language: 'en', persist: false, offer: false});
    expect(decideLanguage('', true, 'zh-CN')).toEqual({language: 'zh-CN', persist: true, offer: false});
    expect(decideLanguage(undefined, true, 'en')).toEqual({language: 'en', persist: false, offer: false});
    expect(decideLanguage(undefined, false, 'zh-CN')).toEqual({language: 'en', persist: false, offer: true});
    expect(decideLanguage('', false, 'en')).toEqual({language: 'en', persist: false, offer: false});
  });
  it('counts a start as the first only without guide flags and without tool paths', () => {
    expect(isFirstStart({guideSeen: false, guideDone: false}, {})).toBe(true);
    expect(isFirstStart({guideSeen: true, guideDone: false}, {})).toBe(false);
    expect(isFirstStart({guideSeen: false, guideDone: true}, {})).toBe(false);
    expect(isFirstStart({guideSeen: false, guideDone: false}, {deadlock: 'D:\\Deadlock'})).toBe(false);
    expect(toolsSet({deadlock: '  ', blender: ''})).toBe(false);
  });
  it('captures the guide flags once, before the app marks the guide seen', () => {
    localStorage.removeItem('dmb.setupGuideSeen');
    forgetLaunch();
    expect(launchFacts().guideSeen).toBe(false);
    localStorage.setItem('dmb.setupGuideSeen', '1');
    expect(launchFacts().guideSeen).toBe(false);
  });
  it('loads the mirrored language before the first render, and Chinese on a first start under Chinese Windows', async () => {
    localStorage.setItem(MIRROR_KEY, 'zh-CN');
    await startLanguage();
    expect(getLanguage()).toBe('zh-CN');
    expect(t('Detect tools')).toBe('检测工具');
    await setLanguage('en');
    localStorage.removeItem(MIRROR_KEY);
    localStorage.removeItem('dmb.setupGuideSeen');
    forgetLaunch();
    Object.defineProperty(navigator, 'language', {configurable: true, get: () => 'zh-CN'});
    try { await startLanguage(); } finally { Reflect.deleteProperty(navigator, 'language'); }
    expect(getLanguage()).toBe('zh-CN');
  });
});
