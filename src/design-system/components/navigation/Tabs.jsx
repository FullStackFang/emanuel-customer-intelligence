import React from 'react';

/**
 * Tabs — horizontal tab bar with the brand's underline-on-active treatment
 * (active tab gets primary-50 fill + a 2px sapphire underline). Controlled via
 * `value` / `onChange`, or uncontrolled with `defaultValue`.
 */
export function Tabs({ items = [], value, defaultValue, onChange, style = {} }) {
  const [internal, setInternal] = React.useState(defaultValue ?? (items[0] && items[0].value));
  const active = value !== undefined ? value : internal;
  const select = (v) => { if (value === undefined) setInternal(v); onChange && onChange(v); };

  return (
    <div
      role="tablist"
      style={{
        display: 'flex',
        gap: 'var(--space-2)',
        borderBottom: '1px solid var(--border-default)',
        paddingBottom: 'var(--space-1)',
        ...style,
      }}
    >
      {items.map((it) => {
        const isActive = it.value === active;
        return (
          <button
            key={it.value}
            role="tab"
            aria-selected={isActive}
            onClick={() => select(it.value)}
            style={{
              position: 'relative',
              display: 'inline-flex',
              alignItems: 'center',
              gap: 'var(--space-2)',
              padding: 'var(--space-2) var(--space-4)',
              fontSize: 'var(--text-sm)',
              fontWeight: isActive ? 'var(--font-semibold)' : 'var(--font-medium)',
              color: isActive ? 'var(--color-primary-600)' : 'var(--text-tertiary)',
              background: isActive ? 'var(--color-primary-50)' : 'transparent',
              border: 'none',
              borderRadius: 'var(--radius-lg) var(--radius-lg) 0 0',
              cursor: 'pointer',
              transition: 'var(--transition-all)',
            }}
          >
            {it.label}
            {it.count != null && (
              <span style={{
                fontSize: 'var(--text-2xs)', fontWeight: 'var(--font-semibold)',
                background: isActive ? 'var(--color-primary-100)' : 'var(--color-neutral-200)',
                color: isActive ? 'var(--color-primary-700)' : 'var(--color-neutral-600)',
                borderRadius: 'var(--radius-full)', padding: '1px 7px', lineHeight: 1.5,
              }}>{it.count}</span>
            )}
            {isActive && (
              <span style={{
                position: 'absolute', bottom: -1, left: 0, right: 0, height: 2,
                background: 'var(--color-primary-500)', borderRadius: 'var(--radius-full)',
              }} />
            )}
          </button>
        );
      })}
    </div>
  );
}
