// Small stroke icons (24px grid, currentColor). Decorative: every use sits next
// to visible text or inside a control with its own accessible name.
import type { ReactNode, SVGProps } from 'react';

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function make(paths: ReactNode) {
  return function Icon({ size = 16, ...rest }: IconProps) {
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.8}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
        focusable="false"
        {...rest}
      >
        {paths}
      </svg>
    );
  };
}

export const ServerIcon = make(
  <>
    <rect x="3" y="4" width="18" height="7" rx="2" />
    <rect x="3" y="13" width="18" height="7" rx="2" />
    <path d="M7 7.5h.01M7 16.5h.01" />
  </>,
);
export const TerminalIcon = make(
  <>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="m7 9 3 3-3 3M13 15h4" />
  </>,
);
export const AssistantIcon = make(
  <>
    <path d="M5 4h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H9l-6 3V6a2 2 0 0 1 2-2z" />
    <path d="m8 9 3 3-3 3M14 14h3" />
  </>,
);
export const SparkIcon = make(
  <path d="M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8zM19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z" />,
);
export const PlugIcon = make(<path d="M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4" />);
export const ClockIcon = make(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </>,
);
export const PlusIcon = make(<path d="M12 5v14M5 12h14" />);
export const SearchIcon = make(
  <>
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </>,
);
export const CloseIcon = make(<path d="M6 6l12 12M18 6 6 18" />);
export const KeyIcon = make(
  <>
    <circle cx="8" cy="15" r="4" />
    <path d="m11 12 9-9M17 6l3 3M15 8l2 2" />
  </>,
);
export const ShieldIcon = make(<path d="M12 3 5 6v6c0 4.2 3 7.6 7 9 4-1.4 7-4.8 7-9V6z" />);
export const ShieldAlertIcon = make(
  <>
    <path d="M12 3 5 6v6c0 4.2 3 7.6 7 9 4-1.4 7-4.8 7-9V6z" />
    <path d="M12 8v5M12 16h.01" />
  </>,
);
export const SendIcon = make(<path d="M5 12h13M13 6l6 6-6 6" />);
export const StopIcon = make(<rect x="6" y="6" width="12" height="12" rx="2" />);
export const CheckIcon = make(<path d="m5 12 5 5 9-10" />);
export const AlertIcon = make(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v6M12 16.5h.01" />
  </>,
);
export const RefreshIcon = make(<path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7" />);
export const EditIcon = make(<path d="M4 20h4L19 9l-4-4L4 16zM14 6l4 4" />);
export const ArchiveIcon = make(
  <>
    <rect x="3" y="4" width="18" height="4" rx="1" />
    <path d="M5 8v11h14V8M10 12h4" />
  </>,
);
export const TrashIcon = make(<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" />);
export const AtIcon = make(
  <>
    <circle cx="12" cy="12" r="4" />
    <path d="M16 12v1.5a2.5 2.5 0 0 0 5 0V12a9 9 0 1 0-3.5 7.1" />
  </>,
);
export const BoltIcon = make(<path d="M13 3 4 14h7l-1 7 9-11h-7z" />);
export const LockIcon = make(
  <>
    <rect x="5" y="11" width="14" height="10" rx="2" />
    <path d="M8 11V7a4 4 0 0 1 8 0v4" />
  </>,
);
export const ChevronIcon = make(<path d="m9 6 6 6-6 6" />);
export const PinIcon = make(
  <>
    <path d="M12 17v5" />
    <path d="M5 9.5 7 5h10l2 4.5-3 3v4H8v-4z" />
  </>,
);
