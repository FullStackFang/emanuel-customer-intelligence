import React from 'react';

const HEIGHTS = { sm: 'var(--input-height-sm)', md: 'var(--input-height-md)', lg: 'var(--input-height-lg)' };
const FONTS = { sm: 'var(--text-xs)', md: 'var(--text-sm)', lg: 'var(--text-base)' };

/**
 * Input — single-line text field. White fill, hairline border, sapphire focus
 * ring. Set `invalid` for the error state.
 */
export function Input({ size = 'md', invalid = false, disabled = false, style = {}, ...rest }) {
  const [focus, setFocus] = React.useState(false);
  const borderColor = invalid ? 'var(--color-error-500)' : (focus ? 'var(--border-focus)' : 'var(--border-default)');
  const ring = invalid ? '0 0 0 3px var(--color-error-100)' : (focus ? '0 0 0 3px var(--color-primary-100)' : 'none');
  return (
    <input
      disabled={disabled}
      onFocus={(e) => { setFocus(true); rest.onFocus && rest.onFocus(e); }}
      onBlur={(e) => { setFocus(false); rest.onBlur && rest.onBlur(e); }}
      style={{
        width: '100%',
        height: HEIGHTS[size] || HEIGHTS.md,
        padding: 'var(--input-padding)',
        fontFamily: 'var(--font-body)',
        fontSize: FONTS[size] || FONTS.md,
        color: 'var(--text-primary)',
        background: disabled ? 'var(--bg-secondary)' : 'var(--bg-primary)',
        border: `var(--input-border-width) solid ${borderColor}`,
        borderRadius: 'var(--radius-lg)',
        boxShadow: ring,
        outline: 'none',
        transition: 'border-color var(--duration-normal) var(--ease-in-out), box-shadow var(--duration-normal) var(--ease-in-out)',
        cursor: disabled ? 'not-allowed' : 'text',
        ...style,
      }}
      {...rest}
    />
  );
}
