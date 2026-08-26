## ADDED Requirements

### Requirement: ZIP-level fiscal-year attrition aggregates
The system SHALL derive ZIP-level attrition aggregates from normalized five-digit `BillingPostalCode` values in the local Account mirror. For each supported completed fiscal year and eligible ZIP, it SHALL report the starting Membership Household count, completed membership-spell exits, and the attrition rate as exits divided by starting households.

#### Scenario: ZIP+4 is normalized
- **WHEN** a Membership Household has a locally mirrored `BillingPostalCode` of `10024-1234`
- **THEN** its aggregate geography is ZIP `10024` and no raw postal code is returned to the webview

#### Scenario: Exit rate uses the fiscal-year starting population
- **WHEN** a ZIP has 40 Membership Households active at the beginning of FY2026 and 4 completed membership spells end during FY2026
- **THEN** the FY2026 ZIP aggregate reports 40 starting households, 4 exits, and a 10.0 percent attrition rate

#### Scenario: Current postal geography is not presented as historical fact
- **WHEN** staff view a ZIP aggregate for a completed fiscal year
- **THEN** the view identifies the geography as based on the locally mirrored Account snapshot rather than an asserted address at exit

### Requirement: Geographic source capability and privacy suppression
The system SHALL make ZIP attrition unavailable when a usable local `BillingPostalCode` source is absent or withheld, and SHALL exclude any ZIP with fewer than five starting Membership Households before returning aggregate data to the webview.

#### Scenario: Postal source is unavailable
- **WHEN** `BillingPostalCode` is missing, withheld, or contains no normalizable ZIP for the relevant population
- **THEN** Insights displays a geographic-source unavailable state and does not render a zero-valued map

#### Scenario: Sparse ZIP is suppressed
- **WHEN** a ZIP has fewer than five Membership Households at the fiscal-year start
- **THEN** that ZIP's count, rate, and geometry are not returned or displayed

#### Scenario: Aggregate map access does not reveal household identity
- **WHEN** staff load ZIP attrition Insights
- **THEN** the response and map contain only ZIP-level counts and rates, without household names, street addresses, coordinates, pins, or named-access audit events

### Requirement: Offline New York ZIP map
The system SHALL render eligible New York ZIP attrition aggregates against a packaged local boundary asset without a third-party map, tile, geocoding, or runtime network service. It SHALL provide an accessible table containing the same mapped aggregates.

#### Scenario: Staff inspect an eligible New York ZIP
- **WHEN** staff hover or focus an eligible ZIP shape on the selected fiscal-year map
- **THEN** they see its ZIP, attrition rate, exit count, and starting-household count

#### Scenario: Offline map rendering
- **WHEN** the desktop app has no network connection and eligible New York ZIP aggregates are available
- **THEN** the map remains renderable from packaged application assets

#### Scenario: ZIP cannot be mapped to a New York boundary
- **WHEN** an eligible ZIP is outside New York or absent from the packaged New York boundary asset
- **THEN** it is excluded from the map and the UI reports that exclusion without converting it to a different geography
