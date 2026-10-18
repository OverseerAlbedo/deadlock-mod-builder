import type { ReactNode } from "react";
const paths: Record<string, ReactNode> = {
  image: (
    <>
      <rect x="3" y="3" width="18" height="18" rx="3" />
      <circle cx="8" cy="8" r="1.5" />
      <path d="m3 17 6-6 4 4 3-3 5 5" />
    </>
  ),
  audio: (
    <>
      <path d="M9 18V5l11-2v13M9 9l11-2" />
      <ellipse cx="6" cy="18" rx="3" ry="2.5" />
      <ellipse cx="17" cy="16" rx="3" ry="2.5" />
    </>
  ),
  sparkles: (
    <>
      <path d="m12 3 2.4 6.6L21 12l-6.6 2.4L12 21l-2.4-6.6L3 12l6.6-2.4L12 3ZM20 2v4M18 4h4" />
    </>
  ),
  upload: (
    <>
      <path d="M12 16V3m-5 5 5-5 5 5M4 15v5a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-5" />
    </>
  ),
  palette: (
    <>
      <path d="M12 3a9 9 0 1 0 0 18c1.4 0 2-1 1.5-2.2-.6-1.3.3-2.8 1.8-2.8H18a3 3 0 0 0 3-3c0-5.5-4-10-9-10Z" />
      <circle cx="7.5" cy="11" r="1.2" />
      <circle cx="10" cy="7" r="1.2" />
      <circle cx="15" cy="7.5" r="1.2" />
    </>
  ),
  search: (
    <>
      <circle cx="10.5" cy="10.5" r="6.5" />
      <path d="m16 16 5 5" />
    </>
  ),
  refresh: (
    <>
      <path d="M20 8a8 8 0 1 0 1 7M20 3v6h-6" />
    </>
  ),
  arrow: <path d="M4 12h16m-6-6 6 6-6 6" />,
  check: <path d="m5 12 4 4L19 6" />,
  close: <path d="m6 6 12 12M6 18 18 6" />,
  play: <path d="m8 4 12 8-12 8Z" />,
  pause: (
    <>
      <path d="M8 4v16M16 4v16" strokeWidth="4" />
    </>
  ),
  volume: (
    <>
      <path d="m11 4-6 5H2v6h3l6 5ZM15 8a5 5 0 0 1 0 8m3-11a9 9 0 0 1 0 14" />
    </>
  ),
  folder: <path d="M3 5h6l2 3h10v12H3Z" />,
  settings: (
    <>
      <path d="M3 7h18M3 17h18" />
      <circle cx="8" cy="7" r="3" />
      <circle cx="16" cy="17" r="3" />
    </>
  ),
  warning: (
    <>
      <path d="M12 3 2 21h20Z" />
      <path d="M12 9v5m0 3v1" />
    </>
  ),
};
export function ExtraIcon({
  name,
  size = 20,
}: {
  name: string;
  size?: number;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[name] ?? paths.image}
    </svg>
  );
}
