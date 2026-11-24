import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GradientRange, GradientSlider, keyValue, percentUnit, shiftUnit, splitRange } from "./GradientSlider";

function Single({ initial = 1, onCommit = vi.fn(), onDrag = vi.fn() }: { initial?: number; onCommit?: () => void; onDrag?: (active: boolean) => void }) {
  const [value, setValue] = useState(initial);
  return <><GradientSlider label="Saturation" value={value} onChange={setValue} min={0} max={2} step={0.05} fine={0.01} unit={percentUnit} track="linear-gradient(90deg, #000, #fff)" onCommit={onCommit} onDrag={onDrag} /><output data-testid="value">{value}</output></>;
}
const value = () => Number(screen.getByTestId("value").textContent);

describe("Gradient slider", () => {
  it("has a name, a value text and the track it shows", () => {
    render(<Single />);
    const slider = screen.getByRole("slider", { name: "Saturation" });
    expect(slider).toHaveAttribute("aria-valuetext", "100%");
    expect(slider).toHaveAttribute("min", "0");
    expect(slider).toHaveAttribute("max", "2");
    expect(slider.parentElement).toHaveStyle({ "--track": "linear-gradient(90deg, #000, #fff)" });
  });
  it("moves a step with the arrows, the fine step with Shift and to the ends with Home and End", () => {
    const commit = vi.fn();
    render(<Single initial={1.02} onCommit={commit} />);
    const slider = screen.getByRole("slider", { name: "Saturation" });
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(value()).toBe(1.05);
    fireEvent.keyDown(slider, { key: "ArrowRight", shiftKey: true });
    expect(value()).toBe(1.06);
    fireEvent.keyDown(slider, { key: "ArrowLeft" });
    expect(value()).toBe(1.05);
    fireEvent.keyDown(slider, { key: "PageUp" });
    expect(value()).toBe(1.55);
    fireEvent.keyDown(slider, { key: "End" });
    expect(value()).toBe(2);
    fireEvent.keyDown(slider, { key: "ArrowUp" });
    expect(value()).toBe(2);
    fireEvent.keyDown(slider, { key: "Home" });
    expect(value()).toBe(0);
    expect(commit).toHaveBeenCalledTimes(7);
  });
  it("takes a typed value at its exact place; invalid text and Escape put the value back", () => {
    render(<Single />);
    const box = screen.getByLabelText("Saturation value");
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: "137" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(value()).toBe(1.37);
    expect(box).toHaveValue("137%");
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: "lots" } });
    fireEvent.blur(box);
    expect(value()).toBe(1.37);
    expect(box).toHaveValue("137%");
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: "500" } });
    fireEvent.keyDown(box, { key: "Escape" });
    expect(box).toHaveValue("137%");
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: "500%" } });
    fireEvent.blur(box);
    expect(value()).toBe(2);
  });
  it("reports a drag and snaps dragged values to the step", () => {
    const drag = vi.fn();
    render(<Single onDrag={drag} />);
    const slider = screen.getByRole("slider", { name: "Saturation" });
    fireEvent.pointerDown(slider);
    expect(drag).toHaveBeenLastCalledWith(true);
    fireEvent.change(slider, { target: { value: "1.23" } });
    expect(value()).toBe(1.25);
    fireEvent.pointerUp(window);
    expect(drag).toHaveBeenLastCalledWith(false);
    // Not dragging: a value set by the input (assistive technology) keeps its place.
    fireEvent.change(slider, { target: { value: "1.23" } });
    expect(value()).toBe(1.23);
  });
  it("computes key values without float drift", () => {
    expect(keyValue("ArrowRight", false, 0.1, 0, 1, 0.05, 0.01)).toBe(0.15);
    expect(keyValue("ArrowLeft", false, 0.15, 0, 1, 0.05, 0.01)).toBe(0.1);
    expect(keyValue("ArrowRight", false, -3, -180, 180, 1, 1)).toBe(-2);
    expect(keyValue("Tab", false, 0, 0, 1, 1, 1)).toBeNull();
    expect(shiftUnit.format(-30)).toBe("−30°");
    expect(shiftUnit.parse("−30°")).toBe(-30);
    expect(shiftUnit.parse("+12")).toBe(12);
  });
});

function Range({ initial = [0.2, 0.8] as [number, number] }) {
  const [values, setValues] = useState(initial);
  return <><GradientRange label="Saturation" name="Range 1 saturation" rangeLabel="range" values={values} onChange={setValues} min={0} max={1} step={0.01} unit={percentUnit} track="linear-gradient(90deg, #888, #f00)" /><output data-testid="range">{values.join(",")}</output></>;
}
const range = () => screen.getByTestId("range").textContent;

describe("Two-handle gradient slider", () => {
  it("names both handles, keeps them in order and moves both when one passes the other", () => {
    render(<Range />);
    const from = screen.getByRole("slider", { name: "Range 1 saturation from" });
    const to = screen.getByRole("slider", { name: "Range 1 saturation to" });
    expect(from).toHaveAttribute("aria-valuetext", "20%");
    expect(to).toHaveAttribute("aria-valuetext", "80%");
    fireEvent.change(from, { target: { value: "0.9" } });
    expect(range()).toBe("0.9,0.9");
    fireEvent.change(to, { target: { value: "0.3" } });
    expect(range()).toBe("0.3,0.3");
    fireEvent.keyDown(to, { key: "End" });
    expect(range()).toBe("0.3,1");
    fireEvent.keyDown(from, { key: "ArrowLeft", shiftKey: true });
    expect(range()).toBe("0.29,1");
  });
  it("takes a typed range in any order and refuses other text", () => {
    render(<Range />);
    const box = screen.getByLabelText("Range 1 saturation range value");
    expect(box).toHaveValue("20–80%");
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: "90 - 10%" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(range()).toBe("0.1,0.9");
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: "half" } });
    fireEvent.blur(box);
    expect(range()).toBe("0.1,0.9");
    expect(splitRange("20–100%")).toEqual(["20", "100%"]);
    expect(splitRange("-30 - 20")).toEqual(["-30", "20"]);
    expect(splitRange("5 to 9")).toEqual(["5", "9"]);
  });
});
