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

interface InstallerStatus {
  state: "not-installed" | "installing" | "installed" | "update-available" | "error";
  installedVersion: string | null;
  targetVersion: string;
  message: string | null;
}

interface ExternalMcpConfiguration {
  clientId: string;
  serverName: string;
  scope: "user" | "project";
  command: string;
  packageSpec: string | null;
  managedByFigcon: boolean;
}

function App() {
  const [environment, setEnvironment] = useState<EnvironmentInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [installerStatus, setInstallerStatus] = useState<InstallerStatus | null>(null);
  const [installerError, setInstallerError] = useState<string | null>(null);
  const [externalConfigs, setExternalConfigs] = useState<ExternalMcpConfiguration[]>([]);
  const [discoveryError, setDiscoveryError] = useState<string | null>(null);
  const [discoveryLoading, setDiscoveryLoading] = useState(true);



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

    invoke<InstallerStatus>("get_installation_status")
      .then((result) => {
        if (active) setInstallerStatus(result);
      })
      .catch((err: unknown) => {
        if (active) setInstallerError(String(err));
      });

    invoke<ExternalMcpConfiguration[]>("discover_claude_code_mcp")
      .then((result) => {
        if (active) setExternalConfigs(result);
      })
      .catch((err: unknown) => {
        if (active) setDiscoveryError(String(err));
      })
      .finally(() => {
        if (active) setDiscoveryLoading(false);
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
      <section>
        <h2>MCP Installation</h2>
        {installerError && (
          <p role="alert">Installation status failed: {installerError}</p>
        )}
        {installerStatus ? (
          <>
            <p>Status: {installerStatus.state}</p>
            <p>Target version: {installerStatus.targetVersion}</p>
            <p>Installed version: {installerStatus.installedVersion ?? "None"}</p>
            {installerStatus.message && <p>{installerStatus.message}</p>}
          </>
        ) : (
          !installerError && <p>Checking installation status...</p>
        )}
      </section>
      <section>
        <h2>Existing Figma Console MCP Configurations</h2>

        {discoveryLoading && <p>Discovering configurations...</p>}

        {discoveryError && (
          <p role="alert">Discovery failed: {discoveryError}</p>
        )}

        {!discoveryLoading && !discoveryError && externalConfigs.length === 0 && (
          <p>No external configurations detected.</p>
        )}

        {externalConfigs.map((config) => (
          <div key={`${config.clientId}-${config.serverName}`}>
            <p>Client: {config.clientId}</p>
            <p>Server: {config.serverName}</p>
            <p>Scope: {config.scope}</p>
            <p>Command: {config.command}</p>
            <p>Package: {config.packageSpec ?? "Unknown"}</p>
            <p>Managed by Figcon: {config.managedByFigcon ? "Yes" : "No"}</p>
          </div>
        ))}
      </section>
    </main>
  );
}

export default App;
