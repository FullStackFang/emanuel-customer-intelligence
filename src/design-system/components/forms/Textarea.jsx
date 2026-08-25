import React from 'react';

/**
 * Textarea — multi-line text field. Same chrome as Input, vertically resizable,
 * min 100px tall.
 */
export function Textarea({ invalid = false, disabled = false, rows = 4, style = {}, ...rest }) {
  const [focus, setFocus] = React.useState(false);
  const borderColor = invalid ? 'var(--color-error-500)' : (focus ? 'var(--border-focus)' : 'var(--border-default)');
  const ring = invalid ? '0 0 0 3px var(--color-error-100)' : (focus ? '0 0 0 3px var(--color-primary-100)' : 'none');
  return (
    <textarea
      rows={rows}
      disabled={disabled}
      onFocus={(e) => { setFocus(true); rest.onFocus && rest.onFocus(e); }}
      onBlur={(e) => { setFocus(false); rest.onBlur && rest.onBlur(e); }}
      style={{
        width: '100%',
        minHeight: 100,
        padding: 'var(--input-padding)',
        fontFamily: 'var(--font-body)',
        fontSize: 'var(--text-sm)',
        lineHeight: 'var(--leading-normal)',
        color: 'var(--text-primary)',
        background: disabled ? 'var(--bg-secondary)' : 'var(--bg-primary)',
        border: `var(--input-border-width) solid ${borderColor}`,
        borderRadius: 'var(--radius-lg)',
        boxShadow: ring,
        outline: 'none',
        resize: 'vertical',
        transition: 'border-color var(--duration-normal) var(--ease-in-out), box-shadow var(--duration-normal) var(--ease-in-out)',
        ...style,
      }}
      {...rest}
    />
  );
}
