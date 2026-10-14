import '@testing-library/jest-dom/vitest';
import { cleanup, configure } from '@testing-library/react';
import { afterEach, beforeEach } from 'vitest';
afterEach(cleanup);
// Tests start as a returning user; the first-start guide has its own test.
// App tests run in Expert mode (every page visible); Simple mode has its own tests.
beforeEach(() => { try { localStorage.setItem('dmb.setupGuideSeen', '1'); localStorage.setItem('dmb.uiMode', 'expert'); } catch { /* no storage */ } });
// Slow CI machines need longer than the 1 s default for findBy/waitFor.
configure({ asyncUtilTimeout: 5000 });
class ResizeObserverStub { observe() {} unobserve() {} disconnect() {} }
globalThis.ResizeObserver = ResizeObserverStub;
// jsdom has no layout engine; hit testing is provided explicitly by drop tests.
if(typeof document!=='undefined')Object.defineProperty(document, 'elementFromPoint', { configurable:true, writable:true, value: () => null });
