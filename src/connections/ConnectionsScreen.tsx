import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import type { AiClient } from "../client-integration/types.ts";
import type { ConnectionSnapshot } from "./session";
import { canApprove, canPreview, knownWarnings } from "./session";
import { clientNames, clientStatus, describeIssue, warnings } from "./presentation";
import { Mark, Notice } from "../setup/components";
export interface ConnectionsScreenProps {
  snapshot: ConnectionSnapshot; navigation?: ReactNode;
  onRefresh(): void; onPrepare(client: AiClient): void; onConfirm(confirmed: boolean): void;
  onApprove(): void; onReject(): void; onSetup(): void;
}
export function ConnectionsScreen({ snapshot: s, navigation, onRefresh, onPrepare, onConfirm, onApprove, onReject, onSetup }: ConnectionsScreenProps) {
  const heading = useRef<HTMLHeadingElement>(null);
  const result = useRef<HTMLDivElement>(null);
  const planId = s.plan?.planId;
  useEffect(() => { if (planId) heading.current?.focus(); }, [planId]);
  useEffect(() => { if (s.outcome && !s.plan) result.current?.focus(); }, [s.outcome, s.plan]);
  const busy = s.action !== "idle";
  const problem = s.issue ? describeIssue(s.issue) : null;
  const inspectionProblem = s.inspectionIssue ? describeIssue(s.inspectionIssue) : null;
  const readiness = s.prerequisites.state;
  const warningUnknown = s.plan?.warnings.some(w => !knownWarnings.includes(w));
  return <main className="app-shell">
    <header className="app-header"><a className="brand" href="#clients" aria-label="Figcon clients"><Mark small/><span>figcon<span className="brand-period">.</span></span></a><span className="platform-label">Private setup</span></header>
    {navigation}
    <section className="connections" id="clients" aria-labelledby="clients-title">
      <div className="intro"><span className="eyebrow">FIGMA CONSOLE / AI CLIENTS</span><h1 id="clients-title">Bring your tools together.</h1><p className="intro-description">Discover your AI apps and review how they could use Figma Console. You stay in control of every proposed change.</p></div>
      <Notice title="Preview mode · Client changes are disabled">You can inspect and review a registration. This version’s backend will block changes, even after approval. Figma plugin onboarding is a separate step.</Notice>
      {readiness !== "ready" && <div className="prerequisite-notice"><Notice title={readiness === "installing" ? "Setup is still in progress" : readiness === "checking" ? "Checking the essentials" : readiness === "invalid" ? "Existing setup needs review" : readiness === "unavailable" ? "Setup status is unavailable" : "Finish the essentials first"} tone={readiness === "checking" || readiness === "installing" ? "neutral" : "warning"}>
        {readiness === "installing" ? "Installation continues while you explore this page. Return to setup to follow progress or cancel." : readiness === "checking" ? "Wait for the installer’s verified status before requesting a plan." : readiness === "missing" ? `${s.prerequisites.missing.join(" and ")} must be installed before a plan can be prepared.` : "Return to setup and check its status. Figcon won’t replace incomplete installations."}
      </Notice><button type="button" className="button-text" onClick={onSetup}>Go to setup<span aria-hidden="true"> ↗</span></button></div>}
      <div className="section-toolbar"><h2>Your AI clients</h2><button type="button" className="button-text" onClick={onRefresh} disabled={busy}>{s.action === "refreshing" ? "Refreshing…" : "Refresh clients"}</button></div>
      {inspectionProblem && <Notice title={inspectionProblem.title} tone="error" alert>{inspectionProblem.body}</Notice>}
      {!s.loaded && !s.inspectionIssue && <div className="client-loading" role="status"><span className="spinner" aria-hidden="true"/>Looking for Claude Code and Codex…</div>}
      <div className="client-grid">
        {(["claude-code", "codex-cli"] as const).map(client => {
          const report = s.clients.find(c => c.client === client);
          const status = report ? clientStatus(report) : null;
          return <article className="client-card" key={client} aria-labelledby={`${client}-title`}>
            <div className="client-card-heading"><span className={`client-monogram ${client}`} aria-hidden="true">{client === "claude-code" ? "C" : "O"}</span><div><h3 id={`${client}-title`}>{clientNames[client]}</h3><p>{client === "claude-code" ? "Anthropic · Claude Code CLI" : "OpenAI · Codex CLI"}</p></div></div>
            <span className={`client-detection${report?.detection === "detected" ? " detected" : ""}`}>{report ? report.detection === "detected" ? "Installed" : report.detection === "missing" ? "Not detected" : "Needs review" : "Not checked"}</span>
            <h4>{status?.title ?? "Waiting for inspection"}</h4><p className="client-description">{status?.body ?? "Refresh to inspect this client without starting it."}</p>
            {report?.warnings.includes("possible-shadowing") && <p className="client-inline-warning">Project settings may take priority over this registration.</p>}
            <button type="button" className="button-secondary" onClick={() => onPrepare(client)} disabled={!canPreview(s, client)}>{s.action === "preparing" && s.selected === client ? "Preparing preview…" : `Review ${clientNames[client]}`}</button>
          </article>;
        })}
      </div>
      {s.recovery && <div className="recovery-summary" role={s.recovery.state === "available" ? undefined : "status"}>
        <span className="recovery-indicator" aria-hidden="true">i</span><div><h3>{s.recovery.state === "full" ? "Recovery storage allowance reached" : s.recovery.state === "unknown" ? "Recovery files couldn’t be checked" : s.recovery.artifactCount ? "Recovery files are being kept" : "Recovery storage checked"}</h3>
          <p>{s.recovery.artifactCount !== null ? `${s.recovery.artifactCount} retained artifacts` : "Retained files need review"}{s.recovery.storageBytes !== null ? ` · ${(s.recovery.storageBytes / 1024 / 1024).toFixed(1)} MB` : ""}. {s.recovery.state !== "available" ? "New plans are blocked until capacity can be checked and files reviewed. " : ""}Nothing is deleted automatically.</p></div>
      </div>}
      {s.recoveryIssue && <Notice title="Recovery storage is unavailable" tone="warning">Refresh after setup is complete. Figcon needs to check retained files before preparing a plan.</Notice>}
      {problem && <div ref={result} tabIndex={-1} className="connection-result"><Notice title={problem.title} tone={s.outcome === "blocked" ? "warning" : "error"} alert>{problem.body}</Notice></div>}
      {s.outcome === "rejected" && !problem && <div ref={result} tabIndex={-1} className="connection-result"><Notice title="Plan declined">This plan can’t be approved. Your client settings were left in place.</Notice></div>}
      {s.plan && <section className="connection-review" aria-labelledby="review-title" aria-busy={s.action === "approving" || s.action === "rejecting"} onKeyDown={event => { if (event.key === "Escape" && !busy) { event.preventDefault(); onReject(); } }}>
        <div className="review-heading"><div><span className="eyebrow">PROPOSED REGISTRATION</span><h2 ref={heading} tabIndex={-1} id="review-title">Review {clientNames[s.plan.client]}</h2></div><span className={`plan-expiry${s.planState !== "review" ? " expired" : ""}`} aria-live="off">{s.planState === "expired" ? "Expired" : s.planState === "stale" ? "New review needed" : `Expires in ${Math.floor(s.secondsLeft / 60)}:${String(s.secondsLeft % 60).padStart(2, "0")}`}</span></div>
        <p className="review-description">Add one user-wide registration pointing to Figcon’s private installation. No existing registration will be replaced or removed.</p>
        <dl className="review-facts"><div><dt>Server name</dt><dd><code>{s.plan.proposedServerName}</code></dd></div><div><dt>Applies to</dt><dd>This user, across projects</dd></div><div><dt>Existing registrations affected</dt><dd>None · Addition only</dd></div></dl>
        <details className="technical-details"><summary>Executable and argument details</summary><dl><dt>Executable</dt><dd><code>{s.plan.proposedExecutable}</code></dd><dt>Arguments (in order)</dt><dd><ol>{s.plan.proposedArguments.map((arg, i) => <li key={i}><code>{arg}</code></li>)}</ol></dd><dt>Configuration scope</dt><dd>{s.plan.scope}</dd><dt>Expires</dt><dd><time dateTime={new Date(s.plan.expiresAtUnixMs).toISOString()}>{new Date(s.plan.expiresAtUnixMs).toLocaleTimeString()}</time></dd></dl></details>
        <ul className="plan-warnings">{s.plan.warnings.map((warning, index) => <li key={`${index}-${warning}`}>{warnings[warning] ?? "This plan contains an unrecognized warning. Refresh before continuing."}</li>)}</ul>
        {warningUnknown && <Notice title="This plan needs a fresh check" tone="warning">Approval is unavailable because some warnings aren’t understood by this version.</Notice>}
        <form onSubmit={event => { event.preventDefault(); onApprove(); }}>
          <label className="approval-confirmation"><input type="checkbox" required checked={s.confirmed} disabled={busy || s.planState !== "review" || readiness !== "ready" || warningUnknown} onChange={event => onConfirm(event.target.checked)}/><span>I reviewed this exact registration and understand that changes are blocked in this version.</span></label>
          <div className="review-actions"><button className="button-primary" type="submit" disabled={!canApprove(s)}>{s.action === "approving" ? "Submitting approval…" : "Confirm approval"}</button><button className="button-secondary" type="button" disabled={busy} onClick={onReject}>{s.action === "rejecting" ? "Declining…" : "Decline plan"}</button></div>
          <p className="review-footnote">Approval is a separate action. The backend will block changes in this build. Escape declines this plan.</p>
        </form>
      </section>}
      <p className="connection-footer-note">Inspection doesn’t start an AI client or Figma Console. Registration, a running client session, and a working Figma connection are different steps.</p>
    </section>
    <footer className="app-footer"><span>Made for the way you create.</span><span>Figcon · Early access</span></footer>
  </main>;
}
