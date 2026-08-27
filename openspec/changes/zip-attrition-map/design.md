## Context

Insights derives aggregate lifecycle views from the encrypted local Salesforce mirror and exposes them through fixed Rust commands. `reason_group` now correctly returns primary Exit Outcomes, but `ReasonsChart` still uses an obsolete fixed list of raw labels, folding every valid outcome into "Other." The mirror has no historical address snapshots and Salesforce access is read-only. Its linked billing statements, however, provide a much more complete postal source (`BillingStatement__c.AddressPostalCode__c`) than `Account.BillingPostalCode`, so geography must use those locally mirrored fields without geocoding or sending data outside the device.

The requested map is a New York ZIP-code choropleth. It must remain usable offline, show only aggregates, and make its snapshot-based geographic attribution clear.

## Goals / Non-Goals

**Goals:**

- Keep the resignation chart aligned with the backend's returned primary Exit Outcomes and safe for future categories.
- Present a New York ZIP-level fiscal-year attrition rate, with exit count and eligible starting-household count available in a tooltip and accessible table.
- Use only local mirrored data and local boundary geometry; suppress sparse ZIP aggregates.
- State when the postal-code source is missing and preserve the availability of all unrelated Insights sections.

**Non-Goals:**

- No household markers, addresses, drill-down lists, named exports, or new named-access audit events.
- No geocoding, third-party maps/tiles, runtime network calls, or collection of additional Salesforce data beyond opted-in mirrored billing-statement and Account postal-code fields.
- No claim that a ZIP or attrition association is causal, nor a reconstruction of historical addresses from a current mirror snapshot.
- No nationwide map or non-New York ZIP shapes in this change.

## Decisions

### Decision 1: Use a dynamic series set with deterministic outcome ordering

`ReasonsChart` will derive its series from `ReasonCell.reason` values instead of maintaining a separate list of raw reason labels. Known primary Exit Outcomes use the canonical order `Addressable Churn`, `Conversion Loss`, `Structural Exit`, and `Administrative or Unknown Exit`; unrecognized categories render after them with deterministic ordering and visually distinct colors. The chart no longer synthesizes an `Other` bucket.

- **Why:** the analytical data contract, not a duplicated UI list, defines which categories exist. This fixes the regression and makes a new reporting category visible by default.
- **Alternative considered:** restore the old raw-label `reason_group` strings. Rejected because it would undo the authoritative multi-label/primary-outcome design.
- **Alternative considered:** retain a fixed list and explicitly add outcome labels. Rejected because the next legitimate category change would reproduce the same failure.

### Decision 2: Normalize the latest linked billing-statement postal code locally, with an Account fallback

The mart rebuild will use the normalized five-digit U.S. ZIP from the latest dated `BillingStatement__c` linked through `Account__c` as the household geography. When no linked statement has a normalizable `AddressPostalCode__c`, it will fall back to `Account.BillingPostalCode`. A statement with no usable issue date cannot establish "latest" and therefore does not override the Account fallback. `BilledToOtherAccountId__c` is deliberately ignored: the statement ZIP remains attributed only to its `Account__c` link, rather than silently moving geography to another account. The geography capability is available when either locally mirrored source yields at least one normalizable ZIP. The raw postal code, any street address, and the bill-to-other identifier are never returned to the webview.

- **Why:** Salesforce inspection found normalizable billing-statement postal codes for 2,829 of 2,850 linked accounts (99.3%), materially improving coverage over the sparsely populated Account field. The normalization accepts ZIP+4 input but only retains the first five digits needed for the map.
- **Alternative considered:** use compound address/location fields. Rejected because the existing Salesforce selector excludes address/location types and they expose more location detail than needed.
- **Alternative considered:** geocode street addresses or use a remote map provider. Rejected for privacy, network, reliability, and data-governance reasons.

### Decision 3: Measure fiscal-year attrition by ZIP against the start-of-year household population

For each available recent completed fiscal year and normalized ZIP, the backend will report: households active at the start of the fiscal year, completed membership spells ending during the fiscal year, and `exits / start_households * 100`, rounded consistently with other Insights percentages. A household belongs to the ZIP from the latest linked locally mirrored billing-statement snapshot, with an Account snapshot fallback; the UI labels this as snapshot-based geography. The map defaults to the latest completed fiscal year and lets staff choose another available recent completed fiscal year.

- **Why:** a rate prevents ZIPs with more households from appearing worse solely due to size; the starting population gives a meaningful fiscal-year denominator.
- **Alternative considered:** map only exit counts. Rejected because counts conflate membership concentration with attrition.
- **Alternative considered:** maintain historical address history. Rejected because it is not available in the current local mirror and would be a separate source-data project.

### Decision 4: Render an offline, aggregate-only New York ZIP choropleth with privacy suppression

The desktop bundle will include a simplified New York ZIP boundary asset rendered as SVG. A ZIP is represented only when its starting-household denominator is at least 5; smaller aggregates are excluded from the response and map. Hover/focus displays only ZIP, rate, exit count, and starting households. The accompanying table exposes the same aggregate rows and labels suppressed and non-New York ZIPs as unavailable for the map.

- **Why:** a packaged SVG boundary asset works offline and avoids external services; a denominator threshold protects small groups.
- **Alternative considered:** Leaflet/remote tiles. Rejected because it makes a local desktop analytics view dependent on a third-party network service.
- **Alternative considered:** county aggregation. Rejected because the user selected ZIP codes for more actionable detail.

## Risks / Trade-offs

- [Current Account ZIP can differ from the ZIP at exit] → label the analysis as snapshot-based geography and do not infer a historical location.
- [Billing postal code is not mirrored or is withheld] → return a geography capability/unavailable reason; do not fabricate an empty or zero map.
- [Sparse ZIPs could disclose a small group] → omit every row below five starting households before the webview response.
- [Some normalized member ZIPs are outside New York or absent from the bundled boundary asset] → retain their aggregate only in no map output and report the count excluded from the New York map.
- [ZIP geometry increases the application bundle] → use a simplified, versioned local boundary asset limited to New York; validate it loads without a network connection.
- [Many dynamic reason categories could reduce chart legibility] → deterministic ordering and a bounded, repeatable palette; the table remains the complete accessible representation.

## Migration Plan

1. Add billing-statement-first postal capability detection and normalized ZIP storage during mart rebuild; existing mirrors rebuild on the next Insights refresh, with no source write.
2. Add aggregate ZIP attrition results to the aggregate Insights response, excluding suppressed and invalid ZIPs before serialization.
3. Add the packaged boundary asset, fiscal-year selector, map, and equivalent aggregate table.
4. Replace the obsolete fixed reason series with the returned dynamic groups.
5. Test billing-statement precedence, Account fallback, ignored bill-to-other links, missing/withheld source, malformed and ZIP+4 values, suppression, fiscal-year math, chart series, and offline boundary rendering; run the existing Insights regression suite.

Rollback is code-only: reverting the change removes the map and restores the prior chart behavior. The optional normalized ZIP mart column is derived data in the encrypted local database and is rebuilt or dropped with the existing mart lifecycle.

## Open Questions

None.
