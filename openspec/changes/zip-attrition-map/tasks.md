## 1. Lifecycle chart regression coverage

- [x] 1.1 Add a frontend regression test proving that each returned primary Exit Outcome becomes a distinct reason-chart series, rather than being folded into `Other`.
- [x] 1.2 Replace the fixed raw-reason series definition with deterministic dynamic category ordering and rendering, then make the regression test pass.

## 2. ZIP attrition analytical contract

- [ ] 2.1 Add Rust tests for `BillingPostalCode` capability detection, five-digit and ZIP+4 normalization, malformed values, and withheld/missing source behavior.
- [ ] 2.2 Add Rust tests for fiscal-year starting-household denominators, exit counts, attrition-rate calculation, five-household suppression, and non-New York ZIP handling.
- [ ] 2.3 Extend the household mart rebuild and aggregate Insights response with optional normalized ZIP geography and ZIP-level completed-fiscal-year attrition cells, keeping raw postal codes out of the webview.
- [ ] 2.4 Update Tauri command bindings and TypeScript API types for the aggregate ZIP attrition response and source-capability state.

## 3. Offline New York map

- [x] 3.1 Select and add a versioned, simplified New York ZIP boundary asset with license/provenance documentation; verify it contains no member data and can load offline.
- [ ] 3.2 Add frontend tests for the unavailable state, fiscal-year selection, privacy-suppressed cells, map tooltip/focus values, and equivalent aggregate table.
- [x] 3.3 Build the accessible local SVG choropleth and fiscal-year selector in Insights, with rate color encoding and count/rate tooltip content.
- [x] 3.4 Add clear snapshot-geography, suppression, and unmapped-ZIP explanatory copy to the map section.

## 4. Verification

- [ ] 4.1 Run the focused Rust and frontend regression tests for reason categories, ZIP normalization, attrition math, suppression, API types, and map UI.
- [ ] 4.2 Run the full automated verification suite (`npm run verify`) and address any failures caused by this change.
- [ ] 4.3 Rebuild Insights from a local mirror with a `BillingPostalCode` field and verify only aggregate ZIP data reaches the map; repeat with that field withheld and verify the explicit unavailable state.
- [ ] 4.4 Verify the packaged map renders with network access disabled and that PDF/report mode retains aggregate-only behavior.
