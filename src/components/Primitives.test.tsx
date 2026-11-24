import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Disclosure } from './Primitives';

describe('Disclosure', () => {
  it('reads its title and hint as two words, not run together', () => {
    const { container } = render(<Disclosure title="Model orientation" hint="Optional: turn the model">x</Disclosure>);
    expect(container.querySelector('summary')!.textContent).toBe('Model orientation Optional: turn the model');
    const plain = render(<Disclosure title="Build settings">x</Disclosure>);
    expect(plain.container.querySelector('summary')!.textContent).toBe('Build settings');
  });
});
