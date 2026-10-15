/** The sidebar mark of the two gradient themes: the app's crossed D (the icon's
 * shape) filled with the theme's gradient. The original theme keeps the plain D. */
const GRADIENTS: Record<string, [string, string, string]> = {
  violet: ['#c98cff', '#4a6cff', '#7fe4ff'],
  mint: ['#5ff2c0', '#39b8ff', '#6c5cff'],
};

export function BrandMark({ theme }: { theme: string }) {
  const stops = GRADIENTS[theme];
  if (!stops) return <>D</>;
  const id = `brand-${theme}`;
  return <svg viewBox="0 0 256 256" width="32" height="32" aria-hidden="true">
    <defs><linearGradient id={id} x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor={stops[0]}/><stop offset=".55" stopColor={stops[1]}/><stop offset="1" stopColor={stops[2]}/></linearGradient></defs>
    <rect x="8" y="8" width="240" height="240" rx="48" fill="#15171c"/>
    <path d="M68 59h50c48 0 77 27 77 69s-29 69-77 69H68V59zm28 27v84h23c30 0 48-16 48-42s-18-42-48-42H96z" fill={`url(#${id})`}/>
    <path d="M48 113h63v30H48z" fill={`url(#${id})`}/>
  </svg>;
}
