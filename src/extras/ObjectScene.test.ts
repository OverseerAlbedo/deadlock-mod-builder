import { describe, expect, it } from "vitest";
import type { FitAnswer } from "./objectCatalog";
import type { ObjectFitView } from "./objectTypes";
import { boundsInside, frameBounds } from "./ObjectScene";

const game = { min: [-10, -5, 0], max: [10, 5, 20] } as ObjectFitView["game_bounds"];
const tall = { min: [-4, -4, 0], max: [4, 4, 60] } as ObjectFitView["game_bounds"];
const answer = (yours: boolean, extra: Partial<ObjectFitView> = {}): FitAnswer => ({
  view: { game_bounds: game, ...(yours ? { yours_bounds: tall } : {}), ...extra } as ObjectFitView,
  game: [],
  yours: yours ? new ArrayBuffer(4) : null,
});

describe("object view framing", () => {
  it("frames the game's object, the user's object or both, by the mode", () => {
    expect(frameBounds(answer(true), "game")).toEqual(game);
    expect(frameBounds(answer(true), "yours")).toEqual(tall);
    expect(frameBounds(answer(true), "both")).toEqual({ min: [-10, -5, 0], max: [10, 5, 60] });
  });
  it("frames the game's object while there is no object of the user's", () => {
    for (const mode of ["game", "yours", "both"] as const) expect(frameBounds(answer(false), mode)).toEqual(game);
  });
  it("adds the collision only while it is drawn", () => {
    const shaped = answer(false, { collision: [{ shape: "sphere", center: [0, 0, 30], radius: 5 }] } as Partial<ObjectFitView>);
    expect(frameBounds(shaped, "game")).toEqual(game);
    expect(frameBounds(shaped, "game", true)).toEqual({ min: [-10, -5, 0], max: [10, 5, 35] });
  });
  it("tells a box inside the framed one from one that runs out of it", () => {
    expect(boundsInside(game, game)).toBe(true);
    expect(boundsInside({ min: [-1, -1, 1], max: [1, 1, 19] }, game)).toBe(true);
    expect(boundsInside(tall, game)).toBe(false);
  });
});
