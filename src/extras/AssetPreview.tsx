import { ExtraIcon } from "./Icons";
import { AudioPlayer } from "./AudioPlayer";
import { useExtraMedia, useVisible } from "./media";
import type { ExtraKind, ExtraPreviewRequest } from "./models";
import type { CatalogEntry } from "../components/AssetCatalog";

export function AssetThumbnail({
  hero,
  entry,
  revision,
}: {
  hero: string;
  entry: CatalogEntry;
  revision: number;
}) {
  const { ref, visible } = useVisible();
  const { url, loading, error } = useExtraMedia(
    visible && entry.path
      ? {
          hero_path: hero,
          target: entry.target,
          kind: "icons",
          max_dimension: 160,
        }
      : null,
    `${entry.original_sha256 ?? ""}:${revision}`,
  );
  return (
    <div ref={ref} className={`extra-thumbnail ${loading ? "loading" : ""}`}>
      {url ? <img src={url} alt="" /> : <ExtraIcon name="image" size={30} />}{" "}
      {error && (
        <span className="extra-thumbnail-error">Preview unavailable</span>
      )}
    </div>
  );
}
export function AssetPreview({
  request,
  identity,
  label,
  kind,
  volume,
  disabled,
}: {
  request: ExtraPreviewRequest | null;
  identity: string;
  label: string;
  kind: ExtraKind;
  volume?: number;
  disabled: boolean;
}) {
  const { media, url, loading, error } = useExtraMedia(request, identity);
  const groups = media?.details.groups as
    | Record<string, { count: number; classes: string[] }>
    | undefined;
  return (
    <div className={`extra-media extra-media-${kind}`}>
      {loading && (
        <div className="extra-preview-placeholder" role="status">
          <span className="extra-spinner" />
          Loading {label.toLowerCase()}…
        </div>
      )}
      {error && (
        <div className="extra-preview-placeholder extra-error" role="status">
          <ExtraIcon name="warning" size={26} />
          <strong>Preview unavailable</strong>
          <p>{error}</p>
        </div>
      )}
      {!loading && !error && !request && (
        <div className="extra-preview-placeholder">
          <ExtraIcon
            name={
              kind === "icons"
                ? "image"
                : kind === "audio"
                  ? "audio"
                  : "sparkles"
            }
            size={34}
          />
          <span>No file selected</span>
        </div>
      )}
      {url && (
        <div className="extra-image-stage">
          <img src={url} alt={`${label} icon preview`} />
        </div>
      )}
      {media?.mime_type === "audio/wav" && (
        <>
          <AudioPlayer
            bytes={media.bytes}
            label={label.toLowerCase()}
            volume={volume}
            disabled={disabled}
          />
          {Number(media.details.padded_frames) > 0 && (
            <p className="extra-processing">
              Padded with silence to match original timing.
            </p>
          )}
          {Number(media.details.trimmed_frames) > 0 && (
            <p className="extra-processing">Trimmed to the original length.</p>
          )}
          {Boolean(
            media.details.resampled || media.details.channels_converted,
          ) && (
            <p className="extra-processing">
              Rate and channels matched automatically.
            </p>
          )}
        </>
      )}
      {groups && (
        <div className="extra-effect-structure">
          <ExtraIcon name="sparkles" size={32} />
          <strong>Particle structure</strong>
          <dl>
            {Object.entries(groups).map(([name, group]) => (
              <div key={name} title={group.classes.join(", ")}>
                <dt>{name}</dt>
                <dd>{group.count}</dd>
              </div>
            ))}
          </dl>
          {Boolean(media?.details.replacement_verified) && (
            <span className="extra-pill ready">
              <ExtraIcon name="check" size={14} />
              Compatible structure
            </span>
          )}
        </div>
      )}
    </div>
  );
}
