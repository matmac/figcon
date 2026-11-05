import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface EnvironmentInfo {
  operatingSystem: "macos" | "windows";
  architecture: "arm64" | "x64";
  clients: {
    id: string;
    status: string;
    detectionMethod: string | null;
    version: string | null;
  }[];
}

function App() {
  const [environment, setEnvironment] = useState<EnvironmentInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;

    invoke<EnvironmentInfo>("detect_environment")
      .then((result) => {
        if (active) setEnvironment(result);
      })
      .catch((err: unknown) => {
        if (active) setError(String(err));
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, []);

  return (
    <main className="container">
      <h1>Figcon</h1>
      <p>Environment diagnostics</p>

      {loading && <p>Detecting environment...</p>}

      {error && <p role="alert">Detection failed: {error}</p>}

      {environment && (
        <section>
          <h2>System</h2>
          <p>Operating system: {environment.operatingSystem}</p>
          <p>Architecture: {environment.architecture}</p>

          <h2>AI Clients</h2>
          {environment.clients.length === 0 ? (
            <p>Client detection not implemented yet.</p>
          ) : (
            <ul>
              {environment.clients.map((client) => (
                <li key={client.id}>
                  {client.id}: {client.status}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
    </main>
  );
}

export default App;
