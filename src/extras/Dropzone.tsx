// i18n-area: extras
import { useEffect, useRef, useState } from "react";
import { openFor, PURPOSE_OF_KIND } from "../dialogs";
import { api, basename } from "../api";
import { m, useT } from "../i18n";
import type { ExtraKind } from "./models";
import { specs } from "./models";
import { ExtraIcon } from "./Icons";
import { previewError } from "./media";

/** The page's own refusal (kept English in state, drawn translated); other errors come from the backend. */
const DROP_ONE = m("Drop one replacement file onto the selected asset.");
/** Whole sentences per kind of asset. */
const DROP_HERE: Record<ExtraKind, string> = { icons: m("Drop your icon here"), audio: m("Drop your sound here"), effects: m("Drop your effect here") };
const DIALOG_TITLE: Record<ExtraKind, string> = { icons: m("Choose replacement icon"), audio: m("Choose replacement sound"), effects: m("Choose replacement effect") };

export function ExtraDropzone({
  kind,
  source,
  disabled,
  change,
}: {
  kind: ExtraKind;
  source: string;
  disabled: boolean;
  change: (path: string) => void;
}) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null),
    mounted = useRef(true),
    busy = useRef(false);
  const [checking, setChecking] = useState(false),
    [error, setError] = useState("");
  const latest = useRef({ disabled, change });
  latest.current = { disabled, change };
  const spec = specs[kind];
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  async function commit(paths: string[]) {
    if (latest.current.disabled || busy.current) return;
    if (paths.length !== 1) {
      setError(DROP_ONE);
      return;
    }
    busy.current = true;
    setChecking(true);
    setError("");
    try {
      const path = await api.validateAssetPath(paths[0], spec.sourceKind);
      if (mounted.current && !latest.current.disabled)
        latest.current.change(path);
    } catch (e) {
      if (mounted.current) setError(previewError(e));
    } finally {
      busy.current = false;
      if (mounted.current) setChecking(false);
    }
  }
  const commitRef = useRef(commit);
  commitRef.current = commit;
  useEffect(() => {
    const node = ref.current;
    const listener = (event: Event) =>
      void commitRef.current((event as CustomEvent<string[]>).detail);
    node?.addEventListener("dmb-file-drop", listener);
    return () => node?.removeEventListener("dmb-file-drop", listener);
  }, []);
  async function browse() {
    try {
      const path = await openFor(PURPOSE_OF_KIND[spec.sourceKind], {
        title: t(DIALOG_TITLE[kind]),
        multiple: false,
        filters: [{ name: spec.formats, extensions: [...spec.extensions] }],
      });
      if (typeof path === "string") await commit([path]);
    } catch (e) {
      if (mounted.current) setError(previewError(e));
    }
  }
  return (
    <div
      className={`extra-dropzone ${source ? "has-source" : ""} ${disabled ? "disabled" : ""}`}
      ref={ref}
      data-native-drop={disabled ? undefined : spec.sourceKind}
    >
      <ExtraIcon name={source ? "check" : "upload"} size={26} />
      <strong>
        {checking
          ? t("Checking file…")
          : source
            ? basename(source)
            : t(DROP_HERE[kind])}
      </strong>
      <span>{source ? t("Drop another file to replace it") : spec.formats}</span>
      <button
        className="extra-secondary"
        disabled={disabled || checking}
        onClick={() => void browse()}
      >
        <ExtraIcon name="folder" size={16} />
        {source ? t("Change file") : t("Choose file")}
      </button>
      {error && (
        <p className="extra-error" role="alert">
          {error === DROP_ONE ? t(DROP_ONE) : error}
        </p>
      )}
    </div>
  );
}
