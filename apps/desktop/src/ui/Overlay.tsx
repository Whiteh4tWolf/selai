import { useEffect, useId, type ReactNode } from 'react';
import { CloseIcon } from './icons';

interface DrawerProps {
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
}

/** Closes on Escape. Shared by the drawer and the dialog. */
function useEscape(onClose: () => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
}

/** A panel that slides in from the right edge for create and edit forms. */
export function Drawer({ title, subtitle, onClose, children, footer }: DrawerProps) {
  const titleId = useId();
  useEscape(onClose);
  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <aside className="drawer" role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <header className="drawer-head">
          <div>
            <h2 id={titleId}>{title}</h2>
            {subtitle && <p className="muted">{subtitle}</p>}
          </div>
          <button type="button" className="icon-btn" aria-label="Close panel" onClick={onClose}>
            <CloseIcon />
          </button>
        </header>
        <div className="drawer-body">{children}</div>
        {footer && <footer className="drawer-foot">{footer}</footer>}
      </aside>
    </div>
  );
}

interface DialogProps {
  title: string;
  tone?: 'default' | 'danger';
  icon?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  /** `contained` keeps the dialog inside its positioned parent, such as one terminal pane. */
  contained?: boolean;
}

export function Dialog({ title, tone = 'default', icon, onClose, children, footer, contained }: DialogProps) {
  const titleId = useId();
  useEscape(onClose);
  return (
    <div className={contained ? 'scrim scrim-contained' : 'scrim'}>
      <div className="dialog" data-tone={tone} role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <header className="dialog-head">
          {icon && <span className="dialog-icon">{icon}</span>}
          <h2 id={titleId}>{title}</h2>
        </header>
        <div className="dialog-body">{children}</div>
        {footer && <footer className="dialog-foot">{footer}</footer>}
      </div>
    </div>
  );
}
