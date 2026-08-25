import "./design-system/styles.css";
import { Button, Card, CardHeader, CardTitle } from "./design-system";

export default function App() {
  return (
    <div style={{ padding: "var(--space-8)" }}>
      <Card>
        <CardHeader><CardTitle>Design system smoke</CardTitle></CardHeader>
        <Button onClick={() => alert("ok")}>Primary Button</Button>
      </Card>
    </div>
  );
}
