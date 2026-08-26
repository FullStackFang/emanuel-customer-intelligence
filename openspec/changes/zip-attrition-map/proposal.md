## Why

The resignation-reason chart maintains a separate, fixed list of labels, so a legitimate backend change to primary Exit Outcomes caused every exit to display as "Other." Staff also need a geographic aggregate to see where attrition is concentrated without exposing a household's address or location.

## What Changes

- Make the exit-reason chart render the categories supplied by lifecycle analytics, preserving the primary Exit Outcome order when those outcomes are present and avoiding silent folding of newly returned groups into "Other."
- Add a ZIP-level New York attrition view that reports fiscal-period resignation count, eligible starting-household count, and attrition rate; map color represents rate and tooltips also show the counts.
- Derive a normalized five-digit ZIP only from an available locally mirrored Account ZIP/postal-code field; do not geocode, transmit address data, or display household pins or addresses.
- Suppress ZIP aggregates below a privacy threshold and provide an explicit unavailable state when no usable ZIP source field is mirrored.
- Package the New York ZIP boundary data with the desktop app so the map does not rely on a third-party map or tile service at runtime.

## Capabilities

### New Capabilities
- `zip-attrition-insights`: privacy-preserving ZIP-level attrition aggregates and their offline New York choropleth display.

### Modified Capabilities
- `membership-lifecycle-insights`: exit-outcome visualization must faithfully represent the categories returned by the lifecycle analytical dataset.

## Impact

- **Code (Rust):** lifecycle mart source capability detection, normalized ZIP derivation, ZIP-level aggregate query/response, and tests in `src-tauri/src/insights.rs`; Insights command/API surface as needed.
- **Code (frontend):** `src/pages/InsightsPage.tsx`, `src/pages/insights/charts.tsx`, API types, and a local map component/data asset.
- **Data/privacy:** reads only the encrypted local Salesforce mirror; returns aggregate ZIP statistics only, suppresses small cells, and does not add named access, geocoding, or audit events.
- **Dependencies/assets:** may add a lightweight SVG/GeoJSON rendering dependency or local New York ZIP boundary asset; no runtime network dependency.
