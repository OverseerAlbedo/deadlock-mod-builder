/** One request at a time for the made clip's report or preview: while one runs, only the newest
 * waiting request is kept (an older waiting one is answered with `DROPPED` and never sent), and an
 * answer that is not for the newest request asked is also answered with `DROPPED`. The engine marks
 * an older request it answers late as superseded; the page treats both the same way: it shows
 * nothing for them. */
export const DROPPED = 'dropped';

export class DroppedError extends Error {
  constructor() { super(DROPPED); this.name = 'DroppedError'; }
}
export const isDropped = (error: unknown) => error instanceof DroppedError;

interface Waiting { id: number; run: () => Promise<unknown>; resolve: (value: unknown) => void; reject: (error: unknown) => void }

export class NewestQueue {
  private running = false;
  private waiting: Waiting | null = null;
  private latest = 0;
  /** Ask; the promise settles with the answer, or rejects with a DroppedError when a newer request came. */
  ask<T>(run: () => Promise<T>): Promise<T> {
    const id = ++this.latest;
    return new Promise<T>((resolve, reject) => {
      this.waiting?.reject(new DroppedError());
      this.waiting = { id, run, resolve: resolve as (value: unknown) => void, reject };
      this.next();
    });
  }
  /** Forget what was asked: a waiting request is dropped and a running one's answer too. */
  drop() {
    this.latest++;
    this.waiting?.reject(new DroppedError());
    this.waiting = null;
  }
  private next() {
    if (this.running || !this.waiting) return;
    const { id, run, resolve, reject } = this.waiting;
    this.waiting = null;
    this.running = true;
    void Promise.resolve()
      .then(run)
      .then(
        value => { if (id === this.latest) resolve(value); else reject(new DroppedError()); },
        error => { if (id === this.latest) reject(error); else reject(new DroppedError()); },
      )
      .finally(() => { this.running = false; this.next(); });
  }
}
