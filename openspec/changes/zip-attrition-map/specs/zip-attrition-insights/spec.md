## ADDED Requirements

### Requirement: ZIP-level fiscal-year attrition aggregates
The system SHALL derive ZIP-level attrition aggregates from the normalized five-digit `AddressPostalCode__c` on the latest dated locally mirrored `BillingStatement__c` linked through `Account__c`, falling back to normalized five-digit `BillingPostalCode` values in the local Account mirror. For each supported completed fiscal year and eligible ZIP, it SHALL report the starting Membership Household count, completed membership-spell exits, and the attrition rate as exits divided by starting households.

#### Scenario: ZIP+4 is normalized
- **WHEN** a Membership Household has a latest linked locally mirrored `AddressPostalCode__c` of `10024-1234`
- **THEN** its aggregate geography is ZIP `10024` and no raw postal code is returned to the webview

#### Scenario: Latest linked billing statement takes precedence
- **WHEN** a Membership Household has a normalizable Account ZIP and multiple linked dated billing statements with normalizable postal codes
- **THEN** its aggregate geography uses the postal code from the latest statement rather than the Account ZIP or an older statement

#### Scenario: Account ZIP is the fallback
- **WHEN** a Membership Household has no linked dated billing statement with a normalizable postal code and has a normalizable `BillingPostalCode`
- **THEN** its aggregate geography uses the normalized Account ZIP

#### Scenario: Exit rate uses the fiscal-year starting population
- **WHEN** a ZIP has 40 Membership Households active at the beginning of FY2026 and 4 completed membership spells end during FY2026
- **THEN** the FY2026 ZIP aggregate reports 40 starting households, 4 exits, and a 10.0 percent attrition rate

#### Scenario: Current postal geography is not presented as historical fact
- **WHEN** staff view a ZIP aggregate for a completed fiscal year
- **THEN** the view identifies the geography as based on the locally mirrored billing-statement or Account snapshot rather than an asserted address at exit

### Requirement: Geographic source capability and privacy suppression
The system SHALL make ZIP attrition unavailable when neither the usable local `BillingStatement__c.AddressPostalCode__c` source nor the usable local `Account.BillingPostalCode` fallback is available, and SHALL exclude any ZIP with fewer than five starting Membership Households before returning aggregate data to the webview.

#### Scenario: Postal source is unavailable
- **WHEN** both local postal sources are missing, withheld, or contain no normalizable ZIP for the relevant population
- **THEN** Insights displays a geographic-source unavailable state and does not render a zero-valued map

#### Scenario: Bill-to-other account link is not followed
- **WHEN** a linked billing statement has `BilledToOtherAccountId__c` populated
- **THEN** the statement postal code is attributed only through `Account__c`, and the bill-to-other identifier is neither followed nor returned to the webview

#### Scenario: Sparse ZIP is suppressed
- **WHEN** a ZIP has fewer than five Membership Households at the fiscal-year start
- **THEN** that ZIP's count, rate, and geometry are not returned or displayed

#### Scenario: Aggregate map access does not reveal household identity
- **WHEN** staff load ZIP attrition Insights
- **THEN** the response and map contain only ZIP-level counts and rates, without household names, raw postal codes, street addresses, bill-to-other identifiers, coordinates, pins, or named-access audit events

### Requirement: Offline New York ZIP map
The system SHALL render eligible New York ZIP attrition aggregates against a packaged local boundary asset without a third-party map, tile, geocoding, or runtime network service. It SHALL provide an accessible table containing the same mapped aggregates.

#### Scenario: Manhattan-area viewport
- **WHEN** staff view the ZIP attrition map
- **THEN** its visible extent is centered on Manhattan and limited to an approximately 50-mile radius, rather than the full New York State boundary extent

#### Scenario: Desktop map navigation
- **WHEN** staff need more detail in the Manhattan-area ZIP map
- **THEN** they can zoom with the pointer wheel or keyboard-accessible controls, pan by dragging, and restore the default extent without a network map service

#### Scenario: Map-stage inspection
- **WHEN** staff inspect the Manhattan-area map
- **THEN** the map presents an in-canvas aggregate legend and ZIP inspection panel, with navigation controls docked at the map edge, rather than separating those tools into page chrome

#### Scenario: Manhattan default extent
- **WHEN** staff open or reset the ZIP attrition map
- **THEN** it opens centered closely on Manhattan, and clicking an eligible ZIP selects its aggregate inspection details

#### Scenario: ZIP drill-in
- **WHEN** staff click an eligible ZIP shape
- **THEN** the map selects that ZIP and fits a closer local view around its boundary, preserving the aggregate-only inspector and the ability to reset to Manhattan

#### Scenario: Staff inspect an eligible New York ZIP
- **WHEN** staff hover or focus an eligible ZIP shape on the selected fiscal-year map
- **THEN** they see its ZIP, attrition rate, exit count, and starting-household count

#### Scenario: Offline map rendering
- **WHEN** the desktop app has no network connection and eligible New York ZIP aggregates are available
- **THEN** the map remains renderable from packaged application assets

#### Scenario: ZIP cannot be mapped to a New York boundary
- **WHEN** an eligible ZIP is outside New York or absent from the packaged New York boundary asset
- **THEN** it is excluded from the map and the UI reports that exclusion without converting it to a different geography
