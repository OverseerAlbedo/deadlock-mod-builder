import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, basename } from "../api";
import type { ExtraKind } from "./models";
import { specs } from "./models";
import { ExtraIcon } from "./Icons";
import { previewError } from "./media";

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
      setError("Drop one replacement file onto the selected asset.");
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
      const path = await open({
        title: `Choose replacement ${spec.singular}`,
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
          ? "Checking file…"
          : source
            ? basename(source)
            : `Drop your ${spec.singular} here`}
      </strong>
      <span>{source ? "Drop another file to replace it" : spec.formats}</span>
      <button
        className="extra-secondary"
        disabled={disabled || checking}
        onClick={() => void browse()}
      >
        <ExtraIcon name="folder" size={16} />
        {source ? "Change file" : "Choose file"}
      </button>
      {error && (
        <p className="extra-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
