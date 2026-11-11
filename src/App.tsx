import { useEffect, useState } from "react";
import { installer } from "./installer/tauri";
import { initialSetupSnapshot, SetupSession } from "./setup/session";
import { SetupScreen } from "./setup/SetupScreen";
import "./App.css";

function App() {
  const [snapshot, setSnapshot] = useState(initialSetupSnapshot);
  // Create a fresh controller for each mounted effect (including StrictMode's
  // setup/cleanup probe). Disposal never cancels the production installer.
  const [actions, setActions] = useState<SetupSession | null>(null);
  useEffect(() => {
    const current = new SetupSession(installer);
    const unsubscribe = current.subscribe(() => setSnapshot(current.getSnapshot()));
    setActions(current); current.connect();
    return () => { unsubscribe(); current.dispose(); };
  }, []);
  return <SetupScreen snapshot={snapshot} onStart={() => { void actions?.start(); }} onCancel={() => { void actions?.cancel(); }} onRefresh={() => actions?.connect()}/>;
}
export default App;
