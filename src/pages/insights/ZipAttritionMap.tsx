import boundaries from "../../assets/ny-zcta-boundaries.json";
import type { ZipAttritionCell } from "../../api";

type Position = [number, number];
type Feature = { properties: { ZCTA5CE20: string }; geometry: { type: "Polygon" | "MultiPolygon"; coordinates: Position[][] | Position[][][] } };
const features = (boundaries as unknown as { features: Feature[] }).features;
export const NY_ZCTAS = new Set(features.map((feature) => feature.properties.ZCTA5CE20));
const points = features.flatMap((feature) => feature.geometry.type === "Polygon"
  ? (feature.geometry.coordinates as Position[][]).flat()
  : (feature.geometry.coordinates as Position[][][]).flat(2));
const minLng = Math.min(...points.map(([lng]) => lng));
const maxLng = Math.max(...points.map(([lng]) => lng));
const minLat = Math.min(...points.map(([, lat]) => lat));
const maxLat = Math.max(...points.map(([, lat]) => lat));

const path = (ring: Position[]) => ring.map(([lng, lat], index) => `${index ? "L" : "M"}${((lng - minLng) / (maxLng - minLng) * 800).toFixed(1)},${((maxLat - lat) / (maxLat - minLat) * 520).toFixed(1)}`).join("") + "Z";
const color = (rate: number) => `rgba(59, 110, 184, ${Math.max(0.2, Math.min(0.95, rate / 35))})`;

export function ZipAttritionMap({ rows, fiscalYear }: { rows: ZipAttritionCell[]; fiscalYear: string }) {
  const byZip = new Map(rows.map((row) => [row.zip, row]));
  return <svg viewBox="0 0 800 520" role="img" aria-label={`New York ZIP attrition map for ${fiscalYear}`} style={{ width: "100%", maxHeight: 520, border: "1px solid var(--border-default)", borderRadius: "var(--radius-md)", background: "var(--bg-secondary)" }}>
    {features.map((feature) => {
      const row = byZip.get(feature.properties.ZCTA5CE20);
      if (!row) return null;
      const polygons = feature.geometry.type === "Polygon" ? [feature.geometry.coordinates as Position[][]] : feature.geometry.coordinates as Position[][][];
      const label = `${row.zip}: ${row.attrition_rate}% attrition; ${row.exits} exits from ${row.start_households} starting households`;
      return <g key={row.zip} tabIndex={0} aria-label={label}>
        <title>{label}</title>
        {polygons.map((polygon, index) => <path key={index} d={path(polygon[0])} fill={color(row.attrition_rate)} stroke="var(--bg-primary)" strokeWidth="0.7" />)}
      </g>;
    })}
  </svg>;
}
