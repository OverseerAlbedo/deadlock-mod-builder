import { fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { addEvent, builtText, eventItems, IGNORED_NOTIFICATIONS, MAX_EVENTS, Notifications, readIgnored, shortTime, writeIgnored, type AppEvent, type NotificationItem } from './Notifications';

const result = {prepared: null, compiled: true, vpk: String.raw`C:\Out\pak09_dir.vpk`, installed: String.raw`C:\Deadlock\game\citadel\addons\pak09_dir.vpk`, manifest: null};
const completed = (id: string, operation = 'build'): AppEvent => ({id, at: 0, kind: 'success', operation, title: 'Build completed', project: 'My mod', lines: [builtText(result)], paths: [{label: 'Output', path: result.vpk}], open: {label: 'Open Build & install', page: 'build'}});
const failed = (id: string): AppEvent => ({id, at: 0, kind: 'error', operation: 'build', title: 'Build failed', error: {title: 'Compiler failed', details: 'line 7: unknown material', help: {cause: 'The compiler stopped.', steps: ['Open Logs.'], action: {label: 'Open Settings', page: 'settings'}}}});
const warning: NotificationItem = {id: 'csdk12', kind: 'warning', title: 'Reduced CSDK 12 in use', body: <p>Steps</p>, dismiss: () => undefined};
const none = {go: () => undefined, dismiss: () => undefined, showDetails: () => undefined};
const region = () => screen.getByRole('region', {name: 'Notifications'});
afterEach(() => { vi.restoreAllMocks(); localStorage.removeItem(IGNORED_NOTIFICATIONS); });

describe('Notifications list', () => {
  it('shows one muted line, no Clear and no summary badge when empty', () => {
    render(<Notifications items={[]} clear={() => undefined}/>);
    expect(within(region()).getByText('No notifications')).toBeInTheDocument();
    expect(within(region()).queryByRole('button', {name: 'Clear'})).toBeNull();
    expect(region().querySelector('.notification-summary')).toBeNull();
  });
  it('labels each badge and sums up the most severe state with the counts', () => {
    const items = [warning, ...eventItems([failed('b'), completed('a'), completed('c', 'install')], none)];
    render(<Notifications items={items} clear={() => undefined}/>);
    const rows = within(region()).getAllByRole('button', {expanded: false});
    expect(rows.map(row => row.querySelector('.notification-badge .sr-only')?.textContent)).toEqual(['Warning', 'Error', 'Success', 'Success']);
    expect(rows.map(row => row.querySelector('.notification-badge [aria-hidden]')?.textContent)).toEqual(['!', '✕', '✓', '✓']);
    const summary = region().querySelector('.notification-summary')!;
    expect(summary).toHaveClass('notification-badge-error');
    expect(summary.textContent).toBe('✕1 error, 1 warning, 2 successes');
    expect(rows[1].textContent).toMatch(/Build failed\d\d:\d\d$/);
  });
  it('is collapsed by default and opens only the clicked item', () => {
    render(<Notifications items={[warning, ...eventItems([completed('a')], none)]} clear={() => undefined}/>);
    expect(screen.queryByText('Steps')).toBeNull();
    const row = within(region()).getByRole('button', {name: /Build completed/});
    fireEvent.click(row);
    expect(row).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('The mod was built and installed in the game as pak09_dir.vpk.')).toBeInTheDocument();
    expect(screen.getByText(String.raw`C:\Out\pak09_dir.vpk`)).toBeInTheDocument();
    expect(screen.queryByText('Steps')).toBeNull();
    fireEvent.click(row);
    expect(screen.queryByText('The mod was built and installed in the game as pak09_dir.vpk.')).toBeNull();
  });
  it('shows a failure briefly in the column: the message, the help action and Show full details', () => {
    const go = vi.fn(), showDetails = vi.fn();
    const events = [failed('b')];
    render(<Notifications items={eventItems(events, {go, showDetails, dismiss: () => undefined})} clear={() => undefined}/>);
    fireEvent.click(within(region()).getByRole('button', {name: /Build failed/}));
    expect(within(region()).getByText('Compiler failed')).toHaveClass('notice-clamp');
    // The cause, steps and technical details are on the page, not in the narrow column.
    expect(screen.queryByText('The compiler stopped.')).toBeNull();
    expect(screen.queryByText('line 7: unknown material')).toBeNull();
    fireEvent.click(screen.getByRole('button', {name: 'Open Settings'}));
    fireEvent.click(screen.getByRole('button', {name: 'Show full details'}));
    expect(go).toHaveBeenCalledWith('settings');
    expect(showDetails).toHaveBeenCalledWith(events[0]);
  });
  it('opens and scrolls to a focused item', () => {
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    const items = eventItems([failed('b'), completed('a')], none);
    const view = render(<Notifications items={items} clear={() => undefined}/>);
    view.rerender(<Notifications items={items} clear={() => undefined} focus="b"/>);
    expect(within(region()).getByRole('button', {name: /Build failed/})).toHaveAttribute('aria-expanded', 'true');
    expect(within(region()).getByRole('button', {name: /Build completed/})).toHaveAttribute('aria-expanded', 'false');
    expect(scroll).toHaveBeenCalledWith({block: 'nearest'});
    delete (Element.prototype as Partial<Element>).scrollIntoView;
  });
  it('Dismiss and Clear call back for every kind', () => {
    const dismiss = vi.fn(), clear = vi.fn(), dismissState = vi.fn();
    render(<Notifications items={[{...warning, dismiss: dismissState}, ...eventItems([completed('a')], {...none, dismiss})]} clear={clear}/>);
    const buttons = within(region()).getAllByRole('button', {name: 'Dismiss'});
    expect(buttons).toHaveLength(2);
    fireEvent.click(buttons[0]); fireEvent.click(buttons[1]);
    expect(dismissState).toHaveBeenCalledOnce();
    expect(dismiss).toHaveBeenCalledWith('a');
    fireEvent.click(within(region()).getByRole('button', {name: 'Clear'}));
    expect(clear).toHaveBeenCalledOnce();
  });
});

describe('Notification events and saved choices', () => {
  it('keeps the newest first and at most 20 events; errors accumulate', () => {
    let events: AppEvent[] = [];
    for (let index = 0; index < MAX_EVENTS + 5; index++) events = addEvent(events, failed(`e${index}`));
    expect(events).toHaveLength(20);
    expect(events[0].id).toBe('e24');
    expect(events.at(-1)!.id).toBe('e5');
  });
  it('keeps only the latest success or warning of one operation', () => {
    let events: AppEvent[] = [];
    events = addEvent(events, completed('a'));
    events = addEvent(events, failed('f'));
    events = addEvent(events, completed('i', 'install'));
    events = addEvent(events, completed('b'));
    expect(events.map(event => event.id)).toEqual(['b', 'i', 'f']);
    events = addEvent(events, {...completed('w', 'install'), kind: 'warning'});
    expect(events.map(event => event.id)).toEqual(['w', 'b', 'f']);
  });
  it('writes the time as HH:MM', () => {
    expect(shortTime(new Date(2026, 10, 15, 9, 5).getTime())).toBe('09:05');
  });
  it('reads and writes the ignored list, and a blocked storage changes nothing', () => {
    writeIgnored(['csdk12']);
    expect(localStorage.getItem(IGNORED_NOTIFICATIONS)).toBe('["csdk12"]');
    expect(readIgnored()).toEqual(['csdk12']);
    writeIgnored([]);
    expect(localStorage.getItem(IGNORED_NOTIFICATIONS)).toBeNull();
    localStorage.setItem(IGNORED_NOTIFICATIONS, 'not json');
    expect(readIgnored()).toEqual([]);
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked'); });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked'); });
    expect(readIgnored()).toEqual([]);
    expect(() => writeIgnored(['csdk12'])).not.toThrow();
  });
});
