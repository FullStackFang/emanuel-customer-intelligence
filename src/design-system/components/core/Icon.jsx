import React from 'react';
import { icons } from 'lucide-react';

const toPascal = (s) =>
  String(s).replace(/(^\w|-\w)/g, (m) => m.replace('-', '').toUpperCase());

/**
 * Icon — thin wrapper over the Lucide icon set (the brand's feather-style,
 * 1.5px-stroke, round-cap system).
 *
 * Ported adaptation: the design project rendered from Lucide's UMD build via
 * window.lucide; as first-class source we render lucide-react components.
 * Same API: kebab-case `name` (e.g. "circle-check"), size, strokeWidth.
 *
 * NOTE: importing the full `icons` map keeps dynamic names working but defeats
 * per-icon tree-shaking. If the public /apply bundle needs slimming (phase 3),
 * switch to a curated registry of the icons the app actually uses.
 */
export function Icon({
  name,
  size = 16,
  strokeWidth = 1.5,
  color = 'currentColor',
  className = '',
  style = {},
  ...rest
}) {
  const LucideIcon = icons[toPascal(name)];
  if (!LucideIcon) return null;
  return (
    <LucideIcon
      size={size}
      strokeWidth={strokeWidth}
      color={color}
      className={`emu-icon ${className}`.trim()}
      style={{ display: 'inline-block', flexShrink: 0, ...style }}
      aria-hidden
      {...rest}
    />
  );
}
