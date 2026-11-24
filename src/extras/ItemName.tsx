// Extras → Items: the side panel's heading. For an item that can be renamed the
// heading is always the name field (the saved name, else the game's), with Save
// and Cancel under it. Typing and Cancel write nothing; Save and "Use the game's
// name" each ask the page for one change.
import { useState } from "react";
import { ExtraIcon } from "./Icons";
import { cleanName, MAX_NAME_CHARS, type ItemNameRow } from "./itemNames";

/** Said under the name field. */
export const ITEM_NAME_NOTES = [
  "Shown for every hero while the mod is installed.",
  "The shop sorts items by name, so a renamed item moves.",
  "After a game update that adds items, build the mod again: until then the new items show without a name.",
];
export const USE_GAME_NAME = "Use the game's name";

export interface ItemHeadingProps {
  /** The game's English name of the item. */
  gameName: string;
  /** The saved rename of this item, if any. */
  row: ItemNameRow | null;
  /** Whether the item can be renamed (the game's names file names it). */
  renamable: boolean;
  /** The icon's status (`replacementStatus`). */
  status: string;
  /** Whether project customization is on (a renamed item applies only then). */
  active: boolean;
  disabled: boolean;
  /** Save this name (already checked); returns a sentence when it was refused. */
  save: (name: string) => string | undefined;
  /** Remove the rename. */
  useGame: () => void;
}

export function ItemHeading({ gameName, row, renamable, status, active, disabled, save, useGame }: ItemHeadingProps) {
  // The unsaved draft; null while the field shows the saved name.
  const [draft, setDraft] = useState<string | null>(null);
  const [refused, setRefused] = useState("");
  const shown = row?.name ?? gameName;
  // Any outside change of the saved name (an undo, a redo, Remove all…) puts it
  // into the field and drops an unsaved draft.
  const [base, setBase] = useState(shown);
  if (base !== shown) {
    setBase(shown);
    setDraft(null);
    setRefused("");
  }
  const nameOn = active && row?.enabled !== false;
  const iconPill = status === "Replaced" ? "New icon" : status === "Original" ? (row ? "" : "Original") : status;
  const pills = (
    <div className="items-pills">
      {row && <span className={`extra-pill ${nameOn ? "ready" : ""}`}>{nameOn ? "Renamed" : "Name paused"}</span>}
      {iconPill && <span className={`extra-pill ${status === "Replaced" ? "ready" : ""}`}>{iconPill}</span>}
    </div>
  );

  if (!renamable) {
    return (
      <header className="extra-inspector-heading items-heading">
        <div>
          <span className="extras-eyebrow">SELECTED ITEM</span>
          <div className="items-title-row">
            <h2>{shown}</h2>
          </div>
          {row && <p className="items-subtitle">{gameName} in the game</p>}
        </div>
        {pills}
      </header>
    );
  }

  const value = draft ?? shown;
  const clean = cleanName(value);
  const error = "error" in clean ? clean.error : refused;
  const cleaned = "name" in clean ? clean.name : "";
  // Unchanged: the name shown now, or the game's own name while nothing is saved.
  const unchanged = cleaned === shown;
  const canSave = !disabled && !error && !unchanged;
  const canCancel = value !== shown;
  const length = [...value.trim()].length;
  function cancel() {
    setDraft(null);
    setRefused("");
  }
  function commit() {
    if (!canSave) return;
    const reason = save(cleaned);
    if (reason) setRefused(reason);
    else cancel();
  }
  return (
    <header className="extra-inspector-heading items-heading editing">
      <div>
        <div className="items-eyebrow-row">
          <span className="extras-eyebrow">SELECTED ITEM</span>
          {pills}
        </div>
        <h2 className="sr-only">{shown}</h2>
        <div className="items-name-field">
          <input
            aria-label="Item name"
            value={value}
            spellCheck={false}
            aria-invalid={Boolean(error)}
            aria-describedby={`items-name-count${error ? " items-name-error" : ""}`}
            onChange={(event) => {
              setDraft(event.target.value);
              setRefused("");
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                commit();
              } else if (event.key === "Escape" && canCancel) {
                event.preventDefault();
                event.stopPropagation();
                cancel();
              }
            }}
          />
          <span id="items-name-count" className={`items-name-count ${length > MAX_NAME_CHARS ? "over" : ""}`}>
            {length} / {MAX_NAME_CHARS}
          </span>
        </div>
        {error && (
          <p id="items-name-error" className="items-name-error" role="alert">
            {error}
          </p>
        )}
        <div className="items-name-under">
          <span className="items-subtitle">{gameName} in the game</span>
          <span className="items-name-buttons">
            <button type="button" className="extra-secondary" disabled={!canCancel} onClick={cancel}>
              Cancel
            </button>
            <button type="button" className="extra-primary" disabled={!canSave} onClick={commit}>
              Save
            </button>
          </span>
        </div>
        {row && (
          <button
            type="button"
            className="extra-text-button items-name-reset"
            disabled={disabled}
            onClick={() => {
              useGame();
              cancel();
            }}
          >
            <ExtraIcon name="undo" size={14} />
            {USE_GAME_NAME}
          </button>
        )}
        <ul className="items-name-notes">
          {ITEM_NAME_NOTES.map((note) => (
            <li key={note} className="items-note">
              {note}
            </li>
          ))}
        </ul>
      </div>
    </header>
  );
}
