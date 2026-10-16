import { cloneElement, useId, type ReactElement } from 'react';

interface Props {
  label: string;
  hint?: string;
  /** One input, select or textarea. It receives the id and the hint link. */
  children: ReactElement<{ id?: string; 'aria-describedby'?: string }>;
}

/** A labelled control with an optional hint, kept out of the accessible name. */
export function Field({ label, hint, children }: Props) {
  const id = useId();
  const hintId = `${id}-hint`;
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {cloneElement(children, { id, 'aria-describedby': hint ? hintId : undefined })}
      {hint && (
        <span className="hint" id={hintId}>
          {hint}
        </span>
      )}
    </div>
  );
}
