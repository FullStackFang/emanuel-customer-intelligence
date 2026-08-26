# New York ZCTA boundary asset

`ny-zcta-boundaries.json` is a local, simplified map asset. It contains only
Census ZCTA identifiers and public boundary geometry; it contains no membership
or other application data and loads without a network request.

- Source: U.S. Census Bureau, 2020 Cartographic Boundary File, 1:500,000-scale
  ZCTAs: `https://www2.census.gov/geo/tiger/GENZ2020/shp/cb_2020_us_zcta520_500k.zip`
- Clip source: U.S. Census Bureau, 2020 Cartographic Boundary File, states:
  `https://www2.census.gov/geo/tiger/GENZ2020/shp/cb_2020_us_state_500k.zip`
- Derivation: clip ZCTAs to the `STUSPS = NY` state boundary, preserve only
  `ZCTA5CE20`, and simplify with Mapshaper weighted simplification at 8% while
  preserving shapes.
- Attribution: U.S. Census Bureau. Census ZCTAs are generalized statistical
  areas, not current USPS delivery ZIP boundaries; a valid ZIP can lack a ZCTA.

To regenerate, use the source files above and the command recorded in the
OpenSpec implementation history. Review the generated asset before committing
to confirm it has only `ZCTA5CE20` properties.
