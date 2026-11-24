import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ColorPicker } from "./ColorPicker";
import { RECENT_KEY, toHex } from "./colorMath";
import type { Rgb } from "./colorAdapter";

function Harness({ initial = null as Rgb | null }) {
  const [color, setColor] = useState<Rgb | null>(initial);
  return (
    <>
      <ColorPicker label="Tint" value={color ?? [0.8, 0.2, 0.1]} original={[0.8, 0.2, 0.1]} originalLabel="Game" changed={Boolean(color)} disabled={false} onChange={setColor} onReset={() => setColor(null)} />
      <output data-testid="color">{color ? toHex(color) : "game"}</output>
    </>
  );
}
const shown = () => screen.getByTestId("color").textContent;
beforeEach(() => localStorage.clear());

describe("Tint picker", () => {
  it("shows the game color beside yours and goes back to it", () => {
    render(<Harness initial={[0, 0, 1]} />);
    expect(screen.getByRole("img", { name: "Game #CC331A, yours #0000FF" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Game color" }));
    expect(shown()).toBe("game");
    expect(screen.getByRole("button", { name: "Game color" })).toBeDisabled();
  });
  it("sets a typed hex color and puts invalid text back", () => {
    render(<Harness />);
    const hex = screen.getByLabelText("Tint hex color");
    fireEvent.focus(hex);
    fireEvent.change(hex, { target: { value: "5600d8" } });
    fireEvent.keyDown(hex, { key: "Enter" });
    expect(shown()).toBe("#5600D8");
    expect(hex).toHaveValue("#5600D8");
    fireEvent.focus(hex);
    fireEvent.change(hex, { target: { value: "#12" } });
    fireEvent.blur(hex);
    expect(shown()).toBe("#5600D8");
    expect(hex).toHaveValue("#5600D8");
    // A typed color is a recent color.
    expect(JSON.parse(localStorage.getItem(RECENT_KEY)!)).toEqual(["#5600D8"]);
  });
  it("edits with HSB or RGB sliders, typed values and keys", () => {
    render(<Harness initial={[1, 0, 0]} />);
    expect(screen.getByRole("slider", { name: "Tint hue" })).toHaveAttribute("aria-valuetext", "0°");
    fireEvent.focus(screen.getByLabelText("Tint hue value"));
    fireEvent.change(screen.getByLabelText("Tint hue value"), { target: { value: "120" } });
    fireEvent.keyDown(screen.getByLabelText("Tint hue value"), { key: "Enter" });
    expect(shown()).toBe("#00FF00");
    fireEvent.keyDown(screen.getByRole("slider", { name: "Tint brightness" }), { key: "Home" });
    expect(shown()).toBe("#000000");
    // Black keeps the hue: brightness back up returns the green.
    fireEvent.keyDown(screen.getByRole("slider", { name: "Tint brightness" }), { key: "End" });
    expect(shown()).toBe("#00FF00");
    fireEvent.click(screen.getByRole("radio", { name: "RGB" }));
    expect(screen.getByRole("slider", { name: "Tint green" })).toHaveAttribute("aria-valuetext", "255");
    fireEvent.change(screen.getByRole("slider", { name: "Tint red" }), { target: { value: "128" } });
    expect(shown()).toBe("#80FF00");
    fireEvent.keyDown(screen.getByRole("slider", { name: "Tint blue" }), { key: "ArrowRight", shiftKey: true });
    expect(shown()).toBe("#80FF01");
  });
  it("remembers the color a typed value or a key sets, not the one before", () => {
    render(<Harness initial={[1, 0, 0]} />);
    const box = screen.getByLabelText("Tint hue value");
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: "120" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(JSON.parse(localStorage.getItem(RECENT_KEY)!)[0]).toBe("#00FF00");
    fireEvent.keyDown(screen.getByRole("slider", { name: "Tint brightness" }), { key: "Home" });
    expect(JSON.parse(localStorage.getItem(RECENT_KEY)!)[0]).toBe("#000000");
  });
  it("offers the recent colors and Black", () => {
    localStorage.setItem(RECENT_KEY, JSON.stringify(["#123456", "#ABCDEF"]));
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "Use #ABCDEF" }));
    expect(shown()).toBe("#ABCDEF");
    expect(JSON.parse(localStorage.getItem(RECENT_KEY)!)).toEqual(["#ABCDEF", "#123456"]);
    fireEvent.click(screen.getByRole("button", { name: "Black" }));
    expect(shown()).toBe("#000000");
    expect(screen.getAllByRole("listitem").map((item) => item.querySelector("button")!.title)).toEqual(["#000000", "#ABCDEF", "#123456"]);
  });
  it("copies the hex value", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<Harness initial={[0, 0, 1]} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy #0000FF" }));
    expect(writeText).toHaveBeenCalledWith("#0000FF");
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });
});
