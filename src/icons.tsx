// SF Symbols-style line icons: 20×20 grid, 1.6 stroke, rounded caps.
import type { SVGAttributes } from "preact";

type P = SVGAttributes<SVGSVGElement>;

function Svg(props: P) {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      stroke-width={1.6}
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden
      {...props}
    />
  );
}

export const DriveIcon = (p: P) => (
  <Svg {...p}>
    <path d="M3.2 10.5 5.4 5.6A1.6 1.6 0 0 1 6.9 4.6h6.2a1.6 1.6 0 0 1 1.5 1l2.2 4.9" />
    <rect x="2.6" y="10.4" width="14.8" height="5.4" rx="1.8" />
    <path d="M13.6 13.1h.01M11.4 13.1h.01" stroke-width={2.2} />
  </Svg>
);


export const EjectIcon = (p: P) => (
  <Svg {...p}>
    <path d="M10 3.8 16 11H4z" fill="currentColor" stroke="none" />
    <rect x="4" y="13.4" width="12" height="2.4" rx="0.8" fill="currentColor" stroke="none" />
  </Svg>
);

export const FolderIcon = (p: P) => (
  <Svg {...p}>
    <path d="M2.8 6.2a1.6 1.6 0 0 1 1.6-1.6h3.3l1.7 1.8h6.2a1.6 1.6 0 0 1 1.6 1.6v6.6a1.6 1.6 0 0 1-1.6 1.6H4.4a1.6 1.6 0 0 1-1.6-1.6z" />
  </Svg>
);

export const RefreshIcon = (p: P) => (
  <Svg {...p}>
    <path d="M16.2 9.4A6.3 6.3 0 0 0 4.9 6.1" />
    <path d="M4.4 2.9v3.6H8" />
    <path d="M3.8 10.6a6.3 6.3 0 0 0 11.3 3.3" />
    <path d="M15.6 17.1v-3.6H12" />
  </Svg>
);


export const AppIcon = (p: P) => (
  <Svg {...p}>
    <rect x="3.4" y="3.4" width="13.2" height="13.2" rx="3.4" />
    <path d="M7 8.2 9 10l-2 1.8M10.8 12h2.4" />
  </Svg>
);

export const CheckIcon = (p: P) => (
  <Svg stroke-width={2.4} {...p}>
    <path d="m5.6 10.4 2.9 2.9 5.9-6.6" />
  </Svg>
);

/** Square "stop" glyph: something that will be stopped automatically before ejecting. */
export const StopIcon = (p: P) => (
  <Svg {...p}>
    <rect x="6.4" y="6.4" width="7.2" height="7.2" rx="1.4" fill="currentColor" stroke="none" />
  </Svg>
);

export const DashIcon = (p: P) => (
  <Svg stroke-width={2.4} {...p}>
    <path d="M6.2 10h7.6" />
  </Svg>
);
