import type { ReactNode } from "react";
import type { ComponentReport, InstallationError, InstallationEvent } from "../installer/types.ts";
import { describeError, downloadLabel, names, stages } from "./presentation";
export function Mark({ small = false }: { small?: boolean }) {
  return <span className={`brand-mark${small ? " brand-mark-small" : ""}`} aria-hidden="true"><svg viewBox="0 0 32 32" fill="none"><path d="M9 23V9h14M9 16h11" stroke="currentColor" strokeWidth="3.5" strokeLinecap="round" strokeLinejoin="round"/><circle cx="23" cy="23" r="3.2" fill="currentColor"/></svg></span>;
}
export function StatusSymbol({ state }: { state: "done" | "active" | "waiting" | "error" }) {
  return <span className={`step-symbol ${state}`} aria-hidden="true">{state === "done" ? <svg viewBox="0 0 20 20"><path d="m5 10 3 3 7-7" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"/></svg> : state === "active" ? <span className="spinner"/> : state === "error" ? "!" : <span className="step-dot"/>}</span>;
}
export function InstallationStep({ step, report, event }: { step: number; report: ComponentReport; event: InstallationEvent | null }) {
  const installed = report.diskState === "installed";
  const active = report.state === "installing";
  const failed = report.diskState === "invalid" || (!installed && report.state === "failed");
  const label = installed && !active ? "Installed" : active ? "In progress" : failed ? "Needs attention" : report.state === "cancelled" ? "Stopped" : "Waiting";
  return <li className="installation-step" aria-current={active ? "step" : undefined}>
    <StatusSymbol state={active ? "active" : installed ? "done" : failed ? "error" : "waiting"}/>
    <div className="step-content"><div className="step-name"><span className="step-index">0{step}</span><h2>{names[report.component]}</h2></div>
      <p>{active && event?.component === report.component ? stages[event.stage] : installed ? `Version ${report.installedVersion ?? report.targetVersion}` : report.component === "node" ? "Node.js · Runs the essentials, just for Figcon" : "The bridge between Figma and your AI tools"}</p></div>
    <span className={`step-status ${installed && !active ? "status-done" : ""}`}>{label}</span>
  </li>;
}
export function InstallationProgress({ event, stopping, canCancel, onCancel }: { event: InstallationEvent | null; stopping: boolean; canCancel: boolean; onCancel():void }) {
  const bytes = downloadLabel(event);
  return <div className="progress-panel">
    <div className="progress-toolbar"><div className="progress-heading" role="status" aria-live="polite" aria-atomic="true"><span>{stopping ? "Cancellation requested" : event ? `${names[event.component]} · ${stages[event.stage]}` : "Waiting for the installer"}</span></div>{canCancel ? <button className="cancel-inline" onClick={onCancel}>Cancel setup</button> : <span className="live-dot" aria-hidden="true"/>}</div>
    {bytes ? <><p className="download-count" aria-live="off">{bytes}</p>{event?.totalBytes && event.downloadedBytes !== null ? <progress aria-label="Runtime download" max={event.totalBytes} value={Math.min(event.downloadedBytes, event.totalBytes)}/> : <div className="activity-track" aria-hidden="true"/>}</> : <div className="activity-track" aria-hidden="true"/>}
    <p className="progress-note">{stopping ? "Please wait for the final status before starting again." : "Every step is checked before it becomes active."}</p>
  </div>;
}
export function Notice({ title, children, tone = "neutral", alert = false }: { title: string; children: ReactNode; tone?: "neutral" | "warning" | "error"; alert?: boolean }) {
  return <div className={`notice notice-${tone}`} role={alert ? "alert" : undefined}><span className="notice-icon" aria-hidden="true">{tone === "error" ? "!" : "i"}</span><div><h3>{title}</h3><p>{children}</p></div></div>;
}
export function ErrorNotice({ issue }: { issue: InstallationError }) {
  const text = describeError(issue);
  return <Notice title={text.title} tone={issue.code === "cancelled" || issue.code === "unstable-snapshot" ? "neutral" : "error"} alert={issue.code !== "unstable-snapshot"}>{text.body}</Notice>;
}
