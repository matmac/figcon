import { useEffect, useState } from "react";
import { installer } from "./installer/tauri";
import { initialSetupSnapshot, SetupSession, activeOperation } from "./setup/session";
import { SetupScreen } from "./setup/SetupScreen";
import { aiClientInspector, clientConsent } from "./client-integration/tauri";
import { ConnectionSession, initialConnectionSnapshot, prerequisitesFrom } from "./connections/session";
import { ConnectionsScreen } from "./connections/ConnectionsScreen";
import { Navigation } from "./connections/Navigation";
import type { Page } from "./connections/Navigation";
import "./App.css";

function App() {
  const [page, setPage] = useState<Page>("setup");
  const [connectionSnapshot, setConnectionSnapshot] = useState(initialConnectionSnapshot);
  const [connections, setConnections] = useState<ConnectionSession | null>(null);
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
  useEffect(() => {
    const current = new ConnectionSession(aiClientInspector, clientConsent);
    const unsubscribe = current.subscribe(() => setConnectionSnapshot(current.getSnapshot()));
    setConnections(current);
    return () => { unsubscribe(); current.dispose(); };
  }, []);
  useEffect(() => { connections?.setPrerequisites(prerequisitesFrom(snapshot)); }, [connections, snapshot]);
  useEffect(() => { if (page === "clients") void connections?.refresh(); }, [connections, page]);
  const navigation = <Navigation page={page} onNavigate={setPage} installing={snapshot.working || !!activeOperation(snapshot.report)}/>;
  if (page === "clients") return <ConnectionsScreen snapshot={connectionSnapshot} navigation={navigation} onRefresh={() => { void connections?.refresh(); }} onPrepare={client => { void connections?.prepare(client); }} onConfirm={value => connections?.confirm(value)} onApprove={() => { void connections?.approve(); }} onReject={() => { void connections?.reject(); }} onSetup={() => setPage("setup")}/>;
  return <SetupScreen navigation={navigation} onClients={() => setPage("clients")} snapshot={snapshot} onStart={() => { void actions?.start(); }} onCancel={() => { void actions?.cancel(); }} onRefresh={() => actions?.connect()}/>;
}
export default App;
