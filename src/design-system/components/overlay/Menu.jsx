import React from 'react';
import { Button } from '../core/Button.jsx';
import { Icon } from '../core/Icon.jsx';

/**
 * Menu — a trigger that opens a floating list of actions. Closes on outside
 * click, Escape, or once an item is picked.
 *
 * `trigger({ open, toggle })` renders the control; `items` is a list of
 * `{ key, label, icon?, onSelect, active?, disabled? }` or `{ divider: true }`.
 */
export function Menu({ trigger, items, align = 'left', minWidth = 210 }) {
  const [open, setOpen] = React.useState(false);
  const ref = React.useRef(null);

  React.useEffect(() => {
    if (!open) return undefined;
    const close = (e) => { if (!ref.current?.contains(e.target)) setOpen(false); };
    const onEsc = (e) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', close);
    document.addEventListener('keydown', onEsc);
    return () => {
      document.removeEventListener('mousedown', close);
      document.removeEventListener('keydown', onEsc);
    };
  }, [open]);

  return (
    <div ref={ref} style={{ position: 'relative' }}>
      {trigger({ open, toggle: () => setOpen((v) => !v) })}

      {open && (
        <div role="menu" style={{
          position: 'absolute', top: 'calc(100% + 4px)', [align === 'right' ? 'right' : 'left']: 0, minWidth,
          background: 'var(--bg-primary)', border: '1px solid var(--border-default)',
          borderRadius: 'var(--radius-lg)', boxShadow: 'var(--shadow-lg)',
          padding: 'var(--space-1)', zIndex: 'var(--z-dropdown)',
        }}>
          {items.map((it, i) => (
            it.divider
              ? <div key={`divider-${i}`} role="separator" style={{ height: 1, margin: 'var(--space-1) 0', background: 'var(--border-default)' }} />
              : <MenuItem key={it.key} item={it} onPick={() => { setOpen(false); it.onSelect(); }} />
          ))}
        </div>
      )}
    </div>
  );
}

function MenuItem({ item, onPick }) {
  const [hover, setHover] = React.useState(false);
  const { label, icon = null, active = false, disabled = false } = item;
  return (
    <button role="menuitem" disabled={disabled} onClick={onPick}
      onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}
      style={{
        display: 'flex', alignItems: 'center', gap: 'var(--space-2)', width: '100%',
        height: 36, padding: '0 var(--space-3)', border: 'none', cursor: disabled ? 'not-allowed' : 'pointer',
        opacity: disabled ? 0.5 : 1, borderRadius: 'var(--radius-md)', textAlign: 'left', whiteSpace: 'nowrap',
        fontFamily: 'var(--font-body)', fontSize: 'var(--text-sm)',
        fontWeight: active ? 'var(--font-semibold)' : 'var(--font-medium)',
        color: active ? 'var(--color-primary-600)' : hover && !disabled ? 'var(--text-primary)' : 'var(--text-secondary)',
        background: active ? 'var(--color-primary-50)' : hover && !disabled ? 'var(--bg-secondary)' : 'transparent',
      }}>
      {icon && <Icon name={icon} size={16} />}
      {label}
    </button>
  );
}

/**
 * MenuButton — a design-system Button that opens a Menu. Any extra props
 * (variant, size, disabled, …) go to the Button.
 */
export function MenuButton({ children, items, align = 'right', ...buttonProps }) {
  return (
    <Menu items={items} align={align} trigger={({ open, toggle }) => (
      <Button {...buttonProps} onClick={toggle} aria-expanded={open} aria-haspopup="menu"
        iconRight={<Icon name="chevron-down" size={14} />}>
        {children}
      </Button>
    )} />
  );
}
