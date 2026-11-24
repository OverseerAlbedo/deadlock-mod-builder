import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ColorMaterialList, stateLine, visibleGroups, yourColor } from "./ColorMaterialList";
import type { ColorMaterial } from "./colorAdapter";

const material = (id: string, group: string, values: Partial<ColorMaterial> = {}): ColorMaterial => ({
  id, label: id, group, texture: null, capabilities: { tint: true, hue: false, saturation: true, brightness: true, ranges: true }, notes: {},
  state: "original", values: null, enabled: true, editable: true, blocked: "", originalTint: [1, 1, 1], baseColor: null, missing: false,
  sharedWith: [], allowShared: false, details: [], otherChanges: [], approximate: false, ...values,
});
const many = Array.from({ length: 80 }, (_, i) => material(`part_${String(i).padStart(2, "0")}`, i < 40 ? "body" : "gear", i % 10 === 0 ? { state: "changed", values: { saturation: 1.2 } } : {}));

describe("Colors material list", () => {
  it("says what changed or the state on each row's second line", () => {
    expect(stateLine(material("a", "g", { state: "changed", values: { color: [0, 0, 0], ranges: [{}] } }), "hero")).toBe("Tint · 1 color range");
    expect(stateLine(material("a", "g", { state: "view-only" }), "hero")).toBe("Cannot be recolored");
    expect(stateLine(material("a", "g", { state: "shared", sharedWith: ["x", "y"] }), "hero")).toBe("Also used by 2 other models");
    expect(stateLine(material("a", "g", { missing: true, state: "missing" }), "hero")).toBe("Saved material unavailable");
    expect(stateLine(material("a", "g"), "hero")).toBe("Game colors");
    expect(stateLine(material("a", "g"), "replacement")).toBe("Original colors");
    expect(yourColor(material("a", "g", { values: { color: [0, 1, 0] } }))).toEqual([0, 1, 0]);
    expect(yourColor(material("a", "g", { values: { saturation: 2 } }))).toBeNull();
    expect(yourColor(material("a", "g", { values: { ranges: [{ hue: 0, hue_width: 10, saturation_range: [1, 1], brightness_range: [1, 1], hue_shift: 120 }] } }))?.map((v) => Math.round(v * 255))).toEqual([0, 255, 0]);
  });
  it("filters and searches into groups, each listed once in list order", () => {
    expect(visibleGroups(many, "all", "").map((g) => [g.name, g.items.length])).toEqual([["body", 40], ["gear", 40]]);
    expect(visibleGroups(many, "changed", "").map((g) => g.items.length)).toEqual([4, 4]);
    expect(visibleGroups(many, "all", "PART_7").flatMap((g) => g.items.map((m) => m.id))).toEqual(["part_70", "part_71", "part_72", "part_73", "part_74", "part_75", "part_76", "part_77", "part_78", "part_79"]);
  });
  it("is a listbox with a selected option, pills and group headings, fast with 80 rows", () => {
    const select = vi.fn();
    const started = performance.now();
    render(<ColorMaterialList materials={many} mode="hero" selected="part_10" select={select} filter="all" setFilter={vi.fn()} query="" setQuery={vi.fn()} thumbnails={{}} seen={vi.fn()} />);
    expect(performance.now() - started).toBeLessThan(1500);
    const list = screen.getByRole("listbox", { name: "Materials" });
    expect(within(list).getAllByRole("option")).toHaveLength(80);
    expect(within(list).getByRole("option", { selected: true })).toHaveTextContent("part_10");
    expect(within(list).getByRole("option", { name: /part_20/ })).toHaveTextContent("Changed");
    expect(within(list).getByRole("group", { name: "gear" })).toBeInTheDocument();
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(select).toHaveBeenLastCalledWith("part_11");
    fireEvent.keyDown(list, { key: "End" });
    expect(select).toHaveBeenLastCalledWith("part_79");
    fireEvent.click(within(list).getByRole("option", { name: /part_05/ }));
    expect(select).toHaveBeenLastCalledWith("part_05");
    expect(screen.getByRole("radio", { name: "Changed 8" })).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /^Weapon/ })).not.toBeInTheDocument();
  });
  it("has a Weapon filter with its count when the weapon's materials are known", () => {
    const weapon = { weapon: new Set(["part_40", "part_41", "part_05"]), body: new Set(["part_05", "part_06"]) };
    expect(visibleGroups(many, "weapon", "", weapon).map((g) => [g.name, g.items.map((m) => m.id)])).toEqual([["body", ["part_05"]], ["gear", ["part_40", "part_41"]]]);
    expect(visibleGroups(many, "weapon", "41", weapon).flatMap((g) => g.items.map((m) => m.id))).toEqual(["part_41"]);
    const setFilter = vi.fn();
    const { rerender } = render(<ColorMaterialList materials={many} mode="hero" selected="" select={vi.fn()} filter="weapon" setFilter={setFilter} query="" setQuery={vi.fn()} thumbnails={{}} seen={vi.fn()} weapon={weapon} />);
    expect(screen.getByRole("radio", { name: "Weapon 3" })).toHaveAttribute("aria-checked", "true");
    const list = screen.getByRole("listbox", { name: "Materials" });
    expect(within(list).getAllByRole("option")).toHaveLength(3);
    // A material of both the weapon and the body says so; one of the weapon alone does not.
    expect(within(list).getByRole("option", { name: /part_05/ })).toHaveTextContent("Used by the weapon and the body");
    expect(within(list).getByRole("option", { name: /part_40/ })).not.toHaveTextContent("Used by the weapon and the body");
    fireEvent.click(screen.getByRole("radio", { name: "All 80" }));
    expect(setFilter).toHaveBeenLastCalledWith("all");
    // Without known weapon materials the filter is gone and the list shows everything.
    rerender(<ColorMaterialList materials={many} mode="hero" selected="" select={vi.fn()} filter="weapon" setFilter={setFilter} query="" setQuery={vi.fn()} thumbnails={{}} seen={vi.fn()} />);
    expect(screen.queryByRole("radio", { name: /^Weapon/ })).not.toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "All 80" })).toHaveAttribute("aria-checked", "true");
    expect(within(screen.getByRole("listbox", { name: "Materials" })).getAllByRole("option")).toHaveLength(80);
  });
});
