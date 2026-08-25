# Emanuel Design System

The design system for **Temple Emanu-El's Philanthropic Fund** and its internal tools. It captures the visual language of the *Temple Events Scheduler* (the resource-calendar app) and extends it to the new **Grant Management** portal — one dignified, quiet, institutional aesthetic shared across both products.

> **Design intent.** The Grant Management Requirements doc proposed a different look (navy + Playfair Display). We deliberately did **not** follow that — the brief was to *keep the resource-calendar app's look and feel and extend it*. So the foundations here are the calendar app's: DM Sans, deep-sapphire + warm-gold, warm-stone neutrals, soft generous corners.

> **Port note.** This directory is a first-class source port of the Claude Design project *Emanuel Design System* (`c768da17-9fdd-4c6a-ad22-c1d17af63897`). Components were ported verbatim except: `Icon`, `Alert`, and `EmptyState` render icons via `lucide-react` instead of the design project's Lucide UMD/window bundle, and `ui-kits/grant-management/chrome.jsx` is converted from window-global preview code to ES-module exports. The original four reference screens live at `docs/design-reference/grant_management_screens.jsx`.

## Content fundamentals
The voice is **dignified, plain, and institutional but human** — the register of a 175-year-old congregation, not a startup.

- **Person:** second person for the user ("Your proposal has been received"), first-person plural for the institution sparingly. Address organizations and committee members with respect.
- **Casing:** Title Case for page/section titles and nav; sentence case for helper text, table cells, and body. UPPERCASE only for tiny eyebrow labels and table headers (with wide letter-spacing).
- **Tone:** factual and calm. State what happened and what's next. No hype, no exclamation marks, no cutesy error copy.
- **Emoji:** never. Not in UI, not in email.
- **Numbers & IDs:** money as `$40,000`; proposal IDs as monospace `PGMS-2026-0041`; EINs masked (`13-•••4567`) for GAs.
- **Examples.** Good: *"Submissions close December 1."* · *"Contact the Grants Administrator to reset your password."* Avoid: *"Woohoo! You're all set 🎉"* · *"Oops — something went wrong!"*

## Visual foundations
- **Color.** Primary is **Deep Sapphire** (`--color-primary-500 #3b6eb8`, headers/actions deepen to 600/700). Accent is **Warm Gold** (`--color-accent-500 #eab308`) — the rose-window gold, used for highlights, ring accents, and the "Philanthropic Fund" eyebrow, **never** for small body text. Neutrals are **Warm Stone** (stone-tinted grays, `#fafaf9 → #1c1917`) — warmer and more inviting than pure gray. Semantic ramps: success (forest), warning (amber), error (red), info (sky).
- **Type.** A single humanist sans — **DM Sans** — for both display and body keeps the tools quiet and functional; headings are semibold with tight tracking. **JetBrains Mono** for IDs, template keys, EINs, and field names only. Modular scale, ratio ~1.25.
- **Backgrounds.** No photography in the internal tools — they're functional. App background is warm off-white (`--bg-secondary`); cards are white. The one expressive surface is the **sapphire gradient header** (`--gradient-brand`, 135° `primary-600 → 700`) and the signed-out hero. No textures, no decorative gradients elsewhere.
- **Corners.** Soft and generous: inputs/buttons `--radius-lg` (8px), cards `--radius-xl` (12px), modals `--radius-2xl` (16px), pills `--radius-full`.
- **Cards.** White fill, 1px `--border-default` hairline, `--shadow-sm` at rest. Hoverable cards lift `translateY(-2px)` to `--shadow-md`.
- **Motion.** Restrained. 100/200/300ms, `--ease-in-out` for most, `--ease-out` for entrances. Respects `prefers-reduced-motion`.
- **Layout.** Top chrome, not a sidebar: 64px gradient header + 56px white nav bar with sapphire underline-on-active. Content max-width ~1200px, centered, on warm off-white.
- **No gold text below 18px** (brand rule).

## Iconography
- **System:** feather-style, 1.5px stroke, round caps/joins, `currentColor` — standardized on **Lucide** via `lucide-react` and the `Icon` wrapper (kebab-case names).
- **No emoji, no Unicode glyphs as icons, no PNG icon sprites.** Icons are always inline SVG.
- **Brand mark:** `src/assets/emanuel_logo.png` (the stained-glass rose window with a Star of David center).

## Index

- `styles.css` — the one file consumers import; `@import`s the token layers only.
- `tokens/` — `fonts.css`, `colors.css`, `typography.css`, `spacing.css`, `radii-shadows.css`, `motion.css`, `components.css`.
- `components/`
  - **core** — `Button`, `IconButton`, `Icon`, `Badge`, `StatusBadge`, `Card` (+ `CardHeader`, `CardTitle`, `CardFooter`)
  - **forms** — `Field`, `Input`, `Textarea`, `Select`
  - **feedback** — `Alert`
  - **navigation** — `Tabs`
  - **overlay** — `Modal`
  - **data** — `Table`, `EmptyState`
- `ui-kits/grant-management/` — `AppFrame`, `PageTitle`, `Stat`, `CycleStepper` chrome.
- `index.js` — barrel export of the 15 components.

## Using it

```jsx
import '../design-system/styles.css';
import { Button, Card, Table, StatusBadge } from '../design-system';
```
