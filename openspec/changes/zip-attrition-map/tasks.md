## 1. Lifecycle chart regression coverage

- [x] 1.1 Add a frontend regression test proving that each returned primary Exit Outcome becomes a distinct reason-chart series, rather than being folded into `Other`.
- [x] 1.2 Replace the fixed raw-reason series definition with deterministic dynamic category ordering and rendering, then make the regression test pass.

## 2. ZIP attrition analytical contract

- [x] 2.1 Add Rust tests for `BillingPostalCode` capability detection, five-digit and ZIP+4 normalization, malformed values, and withheld/missing source behavior.
- [x] 2.2 Add Rust tests for fiscal-year starting-household denominators, exit counts, attrition-rate calculation, five-household suppression, and non-New York ZIP handling.
- [x] 2.3 Extend the household mart rebuild and aggregate Insights response with optional normalized ZIP geography and ZIP-level completed-fiscal-year attrition cells, keeping raw postal codes out of the webview.
- [x] 2.4 Update Tauri command bindings and TypeScript API types for the aggregate ZIP attrition response and source-capability state.

## 3. Offline New York map

- [x] 3.1 Select and add a versioned, simplified New York ZIP boundary asset with license/provenance documentation; verify it contains no member data and can load offline.
- [x] 3.2 Add frontend tests for the unavailable state, fiscal-year selection, privacy-suppressed cells, map tooltip/focus values, and equivalent aggregate table.
- [x] 3.3 Build the accessible local SVG choropleth and fiscal-year selector in Insights, with rate color encoding and count/rate tooltip content.
- [x] 3.4 Add clear snapshot-geography, suppression, and unmapped-ZIP explanatory copy to the map section.

## 4. Verification

- [x] 4.1 Run the focused Rust and frontend regression tests for reason categories, ZIP normalization, attrition math, suppression, API types, and map UI.
- [x] 4.2 Run the full automated verification suite (`npm run verify`) and address any failures caused by this change.
- [x] 4.3 Rebuild Insights from a local mirror with a `BillingPostalCode` field and verify only aggregate ZIP data reaches the map; repeat with that field withheld and verify the explicit unavailable state.
- [ ] 4.4 Verify the packaged map renders with network access disabled and that PDF/report mode retains aggregate-only behavior.

## 5. Billing-statement ZIP coverage

- [x] 5.1 Add Rust regression coverage for latest linked billing-statement precedence, Account fallback, unusable statement dates, source capability, and ignored `BilledToOtherAccountId__c`.
- [x] 5.2 Derive geography from the latest dated linked `BillingStatement__c.AddressPostalCode__c`, with `Account.BillingPostalCode` fallback, while retaining only the normalized ZIP in the mart.
- [x] 5.3 Update ZIP source capability and UI snapshot/source copy without exposing raw postal, street, or bill-to-other data.
- [x] 5.4 Run focused and full automated verification for the revised source behavior.

## 6. Manhattan-focused map viewport

- [x] 6.1 Render the ZIP choropleth against an approximately 50-mile Manhattan-centered viewport and cover it with a frontend regression test.

## 7. Desktop map interaction and visual integration

- [x] 7.1 Add desktop zoom, pan, and reset interactions and restyle the local SVG map with the existing product design tokens.

## 8. Map-stage composition

- [x] 8.1 Recompose the ZIP map as a desktop map stage with an in-canvas aggregate inspector, rate legend, and edge-docked controls, inspired by the existing Events map while preserving local aggregate-only behavior.

## 9. Manhattan default extent

- [x] 9.1 Make the map’s default and reset extent a close Manhattan view and allow clicking a ZIP to inspect its aggregate details.

## 10. ZIP drill-in interaction

- [x] 10.1 Fit the local SVG map to a clicked ZIP boundary and keep that ZIP’s aggregate inspector selected.
