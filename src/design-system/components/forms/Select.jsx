import React from 'react';

const HEIGHTS = { sm: 'var(--input-height-sm)', md: 'var(--input-height-md)', lg: 'var(--input-height-lg)' };
const CHEVRON = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%2378716c' d='M6 8L2 4h8z'/%3E%3C/svg%3E";

/**
 * Select — native dropdown styled to match Input, with the brand chevron.
 * Pass <option>s as children, or an `options` array of {value,label}.
 */
export function Select({ size = 'md', invalid = false, disabled = false, options, children, style = {}, ...rest }) {
  const [focus, setFocus] = React.useState(false);
  const borderColor = invalid ? 'var(--color-error-500)' : (focus ? 'var(--border-focus)' : 'var(--border-default)');
  const ring = invalid ? '0 0 0 3px var(--color-error-100)' : (focus ? '0 0 0 3px var(--color-primary-100)' : 'none');
  return (
    <select
      disabled={disabled}
      onFocus={(e) => { setFocus(true); rest.onFocus && rest.onFocus(e); }}
      onBlur={(e) => { setFocus(false); rest.onBlur && rest.onBlur(e); }}
      style={{
        width: '100%',
        height: HEIGHTS[size] || HEIGHTS.md,
        padding: 'var(--space-2) var(--space-8) var(--space-2) var(--space-3)',
        fontFamily: 'var(--font-body)',
        fontSize: 'var(--text-sm)',
        color: 'var(--text-primary)',
        background: `${disabled ? 'var(--bg-secondary)' : 'var(--bg-primary)'} url("${CHEVRON}") no-repeat right var(--space-3) center`,
        backgroundSize: '12px',
        border: `var(--input-border-width) solid ${borderColor}`,
        borderRadius: 'var(--radius-lg)',
        boxShadow: ring,
        outline: 'none',
        appearance: 'none',
        WebkitAppearance: 'none',
        cursor: disabled ? 'not-allowed' : 'pointer',
        transition: 'border-color var(--duration-normal) var(--ease-in-out), box-shadow var(--duration-normal) var(--ease-in-out)',
        ...style,
      }}
      {...rest}
    >
      {options ? options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>) : children}
    </select>
  );
}
