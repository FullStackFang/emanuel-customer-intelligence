# Grant Management — UI kit

An interactive recreation of the internal **Philanthropic Grant Management** portal, built entirely on the Emanuel Design System and carrying the *Temple Events Scheduler* look-and-feel (sapphire gradient header, white top-nav with sapphire underline-on-active, warm-stone surfaces, DM Sans).

## Screens
- **Dashboard** (`Dashboard`) — annual cycle stepper (`planning → … → closed`), fund-accounting stats, committed-by-focus-area bars, reviewer-progress panel.
- **Proposals** (`Proposals`) — tabbed queue (Needs attention / All / Awarded / Rejected), search + status filter, data table with status badges and scores.
- **Proposal Review & Voting** (`ProposalReview`) — proposal content + budget + attachments, a 5-criterion 1–5 scoring panel with weighted total, the 5-option vote, conditions field, committee tally, and a reject-with-reason modal.
- **Organizations** (`Organizations`) — directory table with masked EINs, focus area, prior-cycle history, invite + active/inactive status.

`Committee` and `Reports` render as documented empty states (out of scope for the kit).

## Files
- `chrome.jsx` — `AppFrame` (header + nav), `PageTitle`, `Stat`, `CycleStepper` — ported to ES modules using the local design-system components.
- The original `screens.jsx` (four reference screens + demo data) lives at `docs/design-reference/grant_management_screens.jsx` — it targets the design-project preview environment (window globals) and is kept as build-time reference for phases 4–6, not compiled source.

## Notes
Domain content (statuses, scoring criteria and weights, vote options, rejection reasons, PGMS-YYYY-NNNN IDs) comes from the requirements doc. Visual language is the calendar app's — **not** the navy/Playfair palette the requirements doc suggested, per the brief to extend the existing product's feel. Icons come from `lucide-react` via the design system's `Icon` wrapper.
