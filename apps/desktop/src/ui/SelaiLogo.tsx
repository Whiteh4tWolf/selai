/** The same mark used by the desktop bundle and installer icons. */
export function SelaiLogo({ size = 20 }: { size?: number }) {
  return <img className="selai-logo" src="/selai-mark.svg" width={size} height={size} alt="" aria-hidden="true" />;
}
