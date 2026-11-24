import { describe, expect, it } from "vitest";
import type { Project } from "../models";
import {
  MAX_ITEM_NAMES,
  MAX_NAME_CHARS,
  NAME_EMPTY,
  NAME_LEADING_HASH,
  NAME_ONE_LINE,
  NAME_REFUSED_CHAR,
  NAME_TOO_LONG,
  REFUSED_CHARS,
  cleanName,
  itemNameCounts,
  itemNameOf,
  pausedItemNames,
  savedItemNames,
  withItemName,
  withItemNameEnabled,
  withItemNameRows,
  withoutItemName,
  type ItemNameRow,
} from "./itemNames";

const project = (customization: Project["customization"]) => ({ customization }) as unknown as Project;

describe("item names", () => {
  it("cleans a name with the same rules as the backend", () => {
    expect(cleanName("  Close Call  ")).toEqual({ name: "Close Call" });
    expect(cleanName("\u3000全角\u00a0")).toEqual({ name: "全角" });
    expect(cleanName("\ufeffA")).toEqual({ name: "\ufeffA" }); // not white space, not a control character (as in Rust)
    expect(cleanName("é".repeat(MAX_NAME_CHARS))).toEqual({ name: "é".repeat(MAX_NAME_CHARS) });
    expect(cleanName("😀".repeat(MAX_NAME_CHARS))).toEqual({ name: "😀".repeat(MAX_NAME_CHARS) });
    expect(cleanName("Mid # sign")).toEqual({ name: "Mid # sign" });
    expect(cleanName("   ")).toEqual({ error: NAME_EMPTY });
    expect(cleanName("a".repeat(MAX_NAME_CHARS + 1))).toEqual({ error: NAME_TOO_LONG });
    for (const name of ["two\nlines", "tab\there", "bell\u0007", "c1\u0085x"]) expect(cleanName(name)).toEqual({ error: NAME_ONE_LINE });
    for (const c of REFUSED_CHARS) expect(cleanName(`a${c}b`)).toEqual({ error: NAME_REFUSED_CHAR });
    expect(cleanName("  #a")).toEqual({ error: NAME_LEADING_HASH });
  });

  it("sets, changes, switches off and removes a row, dropping the key with the last one", () => {
    let rows: ItemNameRow[] = [];
    rows = withItemName(rows, "upgrade_sample", " First ").rows;
    expect(rows).toEqual([{ item: "upgrade_sample", enabled: true, name: "First" }]);
    rows = withItemNameEnabled(rows, "UPGRADE_SAMPLE", false);
    rows = withItemName(rows, "Upgrade_Sample", "Second").rows;
    expect(rows).toEqual([{ item: "upgrade_sample", enabled: false, name: "Second" }]); // keeps its spelling and switch
    const refused = withItemName(rows, "upgrade_sample", 'Say "hi"');
    expect(refused).toEqual({ rows, error: NAME_REFUSED_CHAR });
    expect(itemNameOf(rows, "UPGRADE_sample")?.name).toBe("Second");
    expect(pausedItemNames(rows, true)[0].enabled).toBe(true);

    const p = withItemNameRows(project({ enabled: false }), rows, true);
    expect(p.customization.enabled).toBe(true);
    expect(savedItemNames(p)).toEqual(rows);
    expect(itemNameCounts(p)).toEqual({ saved: 1, enabled: 0 });
    const emptied = withItemNameRows(p, withoutItemName(rows, "upgrade_SAMPLE"), false);
    expect("item_names" in emptied.customization).toBe(false);
    expect(savedItemNames(emptied)).toEqual([]);
  });

  it("refuses a row past the limit", () => {
    const many = Array.from({ length: MAX_ITEM_NAMES }, (_, i) => ({ item: `upgrade_${i}`, enabled: true, name: "A" }));
    const more = withItemName(many, "upgrade_new", "B");
    expect(more.rows).toBe(many);
    expect(more.error).toContain(`${MAX_ITEM_NAMES}`);
    expect(withItemName(many, "upgrade_0", "B").rows[0].name).toBe("B");
  });
});
