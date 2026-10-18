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
export const ImportIcon = make(<path d="M12 3v12m-4-4 4 4 4-4M4 16v4h16v-4" />);
export const SearchIcon = make(
  <>
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </>,
);
export const CloseIcon = make(<path d="M6 6l12 12M18 6 6 18" />);
export const MinimizeIcon = make(<path d="M5 17h14" />);
export const MaximizeIcon = make(<rect x="5" y="5" width="14" height="14" rx="1" />);
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
export const FolderIcon = make(<path d="M3 7V5a1 1 0 0 1 1-1h6l2 3h8a1 1 0 0 1 1 1v11H3z" />);
export const ForwardIcon = make(<><path d="M4 7h15m-4-4 4 4-4 4M20 17H5m4-4-4 4 4 4" /><circle cx="4" cy="7" r="1" /><circle cx="20" cy="17" r="1" /></>);
export const ProxyIcon = make(<><circle cx="5" cy="12" r="3" /><circle cx="19" cy="5" r="3" /><circle cx="19" cy="19" r="3" /><path d="m8 11 8-5M8 13l8 5M19 8v8" /></>);
export const FilesIcon = make(<><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M12 4v16M6 8h3M15 8h3M6 12h3M15 12h3" /></>);
export const FileIcon = make(<><path d="M14 3H5v18h14V8zM14 3v6h5M8 13h8M8 17h5" /></>);
export const PlayIcon = make(<path d="m8 4 12 8-12 8z" />);
export const CopyIcon = make(<><rect x="8" y="8" width="12" height="13" rx="2" /><path d="M16 8V3H3v13h5" /></>);
export const UploadIcon = make(<path d="M12 16V3m-4 4 4-4 4 4M4 15v6h16v-6" />);
export const UpIcon = make(<path d="M12 20V4m-6 6 6-6 6 6" />);
export const EyeIcon = make(<><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z" /><circle cx="12" cy="12" r="3" /></>);
export const EyeOffIcon = make(<><path d="m3 3 18 18M10.6 5.1 12 5c6.5 0 10 7 10 7a19 19 0 0 1-3.1 3.8M6.1 6.1A21 21 0 0 0 2 12s3.5 7 10 7a12 12 0 0 0 5.9-1.8M10 10a3 3 0 0 0 4 4" /></>);
export const PinIcon = make(
  <>
    <path d="M12 17v5" />
    <path d="M5 9.5 7 5h10l2 4.5-3 3v4H8v-4z" />
  </>,
);
