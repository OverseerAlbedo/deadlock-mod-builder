import { useEffect, useRef, useState } from "react";
import { api, errorText } from "../api";
import type { ExtraMedia, ExtraPreviewRequest } from "./models";

export function parseExtraMedia(data: ArrayBuffer): ExtraMedia {
  if (data.byteLength < 4) throw new Error("Incomplete asset preview.");
  const size = new DataView(data).getUint32(0, true);
  if (size > 65536 || size + 4 > data.byteLength)
    throw new Error("Invalid asset preview header.");
  const head = JSON.parse(
    new TextDecoder().decode(new Uint8Array(data, 4, size)),
  ) as { mime_type: string; details: Record<string, unknown> };
  if (!["image/png", "image/svg+xml", "audio/wav", "application/json"].includes(head.mime_type))
    throw new Error("Unsupported asset preview.");
  return { ...head, bytes: data.slice(4 + size) };
}

// A small shared queue prevents grids from flooding native workers. Cancelled
// cards are skipped before dispatch. Replacements are never cached by file path.
let running = 0;
const queue: (() => void)[] = [];
const cache = new Map<string, ExtraMedia>();
let cachedBytes = 0;
function requestMedia(
  request: ExtraPreviewRequest,
  key: string,
  signal: AbortSignal,
): Promise<ExtraMedia> {
  if (cache.has(key)) {
    const value = cache.get(key)!;
    cache.delete(key);
    cache.set(key, value);
    return Promise.resolve(value);
  }
  return new Promise((resolve, reject) => {
    const task = () => {
      if (signal.aborted) {
        reject(new DOMException("Preview cancelled", "AbortError"));
        next();
        return;
      }
      running++;
      void Promise.resolve()
        .then(() => api.previewExtra(request))
        .then(parseExtraMedia)
        .then((media) => {
          if (!request.source && ["image/png", "image/svg+xml"].includes(media.mime_type)) {
            const previous = cache.get(key);
            if (previous) cachedBytes -= previous.bytes.byteLength;
            cache.set(key, media);
            cachedBytes += media.bytes.byteLength;
            while (cache.size > 128 || cachedBytes > 24 * 1024 * 1024) {
              const first = cache.keys().next().value!;
              cachedBytes -= cache.get(first)!.bytes.byteLength;
              cache.delete(first);
            }
          }
          resolve(media);
        }, reject)
        .finally(() => {
          running--;
          next();
        });
    };
    queue.push(task);
    next();
  });
}
function next() {
  while (running < 2 && queue.length) queue.shift()!();
}
export function clearExtraMediaCache() {
  cache.clear();
  cachedBytes = 0;
}
export function previewError(error: unknown) {
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string"
  )
    return error.message;
  return errorText(error);
}
export function useExtraMedia(
  request: ExtraPreviewRequest | null,
  identity: string,
) {
  const [state, setState] = useState<{
    key: string;
    media: ExtraMedia | null;
    url: string;
    loading: boolean;
    error: string;
  }>({ key: "", media: null, url: "", loading: false, error: "" });
  const serialized = JSON.stringify(request);
  const key = `${serialized}:${identity}`;
  useEffect(() => {
    const controller = new AbortController();
    let url = "";
    setState({
      key,
      media: null,
      url: "",
      loading: Boolean(request),
      error: "",
    });
    if (request)
      void requestMedia(request, key, controller.signal)
        .then((media) => {
          if (controller.signal.aborted) return;
          if (["image/png", "image/svg+xml"].includes(media.mime_type))
            url = URL.createObjectURL(
              new Blob([media.bytes], { type: media.mime_type }),
            );
          setState({ key, media, url, loading: false, error: "" });
        })
        .catch((error) => {
          if (!controller.signal.aborted)
            setState({
              key,
              media: null,
              url: "",
              loading: false,
              error: previewError(error),
            });
        });
    return () => {
      controller.abort();
      if (url) URL.revokeObjectURL(url);
    };
    // Serialized requests are the stable dependency, not object identity.
  }, [key]);
  return state.key === key
    ? state
    : { key, media: null, url: "", loading: Boolean(request), error: "" };
}
export function useVisible() {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(
    typeof IntersectionObserver === "undefined",
  );
  useEffect(() => {
    if (visible || !ref.current || typeof IntersectionObserver === "undefined")
      return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "120px" },
    );
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, [visible]);
  return { ref, visible };
}
