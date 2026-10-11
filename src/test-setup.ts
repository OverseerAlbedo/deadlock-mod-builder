import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';
afterEach(cleanup);
class ResizeObserverStub { observe() {} unobserve() {} disconnect() {} }
globalThis.ResizeObserver = ResizeObserverStub;
// jsdom has no layout engine; hit testing is provided explicitly by drop tests.
if(typeof document!=='undefined')Object.defineProperty(document, 'elementFromPoint', { configurable:true, writable:true, value: () => null });
