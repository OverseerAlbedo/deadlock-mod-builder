import { describe, expect, it } from "vitest";
import { skinPositions } from "./ObjectScene";
import { clipLine, parseMotion, playsLine, wayOf } from "./ObjectMoving";
import { motionAnswer } from "./objectFixtures";

describe("the player's skinning and motion answers", () => {
  const motion = {
    bones: ["main", "bar"],
    frames: 2,
    // Frame 0: both at rest; frame 1: "bar" moved 18 down (z), "main" turned a quarter about z.
    data: new Float32Array([
      1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0,
      0, -1, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, -18,
    ]),
  };
  const rest = new Float32Array([1, 0, 25, 2, 0, 0]);
  it("moves each point by its joints' matrices and leaves a joint the motion lacks at rest", () => {
    // Point 0 wholly on "bar" (joint 0 of the mesh), point 1 half "main", half a joint the motion lacks.
    const pairs = new Float32Array([0, 1, 0, 0, 1, 0.5, 2, 0.5]);
    const joints = ["bar", "main", "other"];
    expect([...skinPositions(rest, pairs, 2, joints, motion, 0)]).toEqual([1, 0, 25, 2, 0, 0]);
    const moved = [...skinPositions(rest, pairs, 2, joints, motion, 1)];
    expect(moved.slice(0, 3)).toEqual([1, 0, 7]);
    // Half turned (0, 2, 0) and half kept (2, 0, 0).
    expect(moved.slice(3)).toEqual([1, 1, 0]);
    // Frames past the end hold the last one.
    expect([...skinPositions(rest, pairs, 2, joints, motion, 9)].slice(0, 3)).toEqual([1, 0, 7]);
  });
  it("reads a motion answer, and a header without frames gives no motion", () => {
    const { header, motion: parsed } = parseMotion(motionAnswer());
    expect(header.clips.map((c) => c.label)).toEqual(["Deploy", "Drain liquid"]);
    expect(parsed?.frames).toBe(2);
    expect(parsed?.data.length).toBe(2 * 2 * 12);
    expect(wayOf(undefined, 3)).toBe("by_parts");
    expect(wayOf(undefined, 1)).toBe("one_piece");
    expect(wayOf({ way: "one_piece" }, 3)).toBe("one_piece");
  });
  it("says how the game's graph plays each clip", () => {
    const deploy = { file: "d", label: "Deploy", duration: 30, additive: false, loops: false };
    expect(clipLine(deploy, "Medicinal Specter")).toBe("Deploy (30.0 s): the game plays it while Medicinal Specter runs. The game plays it once and holds its last pose.");
    expect(clipLine({ ...deploy, label: "Drain liquid", duration: 0.9667, additive: true, loops: true }, undefined)).toBe("Drain liquid (1.0 s, played over the others): the game plays it while the ability runs. It loops.");
    expect(playsLine({ ...deploy, loops: undefined })).toBe("");
  });
});
