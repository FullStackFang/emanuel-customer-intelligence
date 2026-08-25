import React from 'react';

/**
 * Field — label + control wrapper with optional required marker, hint, and
 * error text. Wrap any Input/Textarea/Select.
 */
export function Field({ label, required = false, hint, error, htmlFor, children, style = {}, ...rest }) {
  return (
    <div style={{ marginBottom: 'var(--space-4)', ...style }} {...rest}>
      {label && (
        <label
          htmlFor={htmlFor}
          style={{
            display: 'block',
            fontSize: 'var(--text-sm)',
            fontWeight: 'var(--font-medium)',
            color: 'var(--text-secondary)',
            marginBottom: 'var(--space-1)',
          }}
        >
          {label}
          {required && <span style={{ color: 'var(--color-error-500)' }}> *</span>}
        </label>
      )}
      {children}
      {hint && !error && (
        <div style={{ fontSize: 'var(--text-xs)', color: 'var(--text-tertiary)', marginTop: 'var(--space-1)' }}>{hint}</div>
      )}
      {error && (
        <div style={{ fontSize: 'var(--text-xs)', color: 'var(--color-error-500)', marginTop: 'var(--space-1)' }}>{error}</div>
      )}
    </div>
  );
}
