import type { SetupSnapshot } from "./session";
import { setupView } from "./presentation";
import { ErrorNotice, InstallationProgress, InstallationStep, Mark, Notice } from "./components";
export interface SetupScreenProps { snapshot: SetupSnapshot; onStart(): void; onCancel(): void; onRefresh(): void }
export function SetupScreen({ snapshot, onStart, onCancel, onRefresh }: SetupScreenProps) {
  const view = setupView(snapshot);
  const { report } = view;
  const retention = report?.retention;
  return <main className="app-shell">
    <header className="app-header"><a className="brand" href="#setup" aria-label="Figcon setup"><Mark small/><span>figcon<span className="brand-period">.</span></span></a><span className="platform-label"><span aria-hidden="true">◈</span> Local setup</span></header>
    <section className="setup" id="setup" aria-labelledby="setup-title">
      <div className="intro"><span className="eyebrow">FIGMA CONSOLE / SETUP</span><h1 id="setup-title">{view.title}</h1><p className="intro-description">{view.description}</p></div>
      <div className="setup-card" aria-label="Installation steps">
        <div className="card-caption"><span>YOUR ESSENTIALS</span><span>02 COMPONENTS</span></div>
        {report ? <ol className="step-list"><InstallationStep step={1} report={report.node} event={view.event}/><InstallationStep step={2} report={report.mcp} event={view.event}/></ol> : <div className="initial-loading" role="status"><span className="spinner" aria-hidden="true"/><span>Checking installed components…</span></div>}
        {view.inProgress && <InstallationProgress event={view.event} stopping={view.stopping} canCancel={view.canCancel} onCancel={onCancel}/>}
        {view.installed && !view.inProgress && <div className="success-summary" role="status"><span className="success-check" aria-hidden="true">✓</span><span>Both components are installed and verified.</span></div>}
      </div>
      <div className="messages">
        {snapshot.syncIssue && <ErrorNotice issue={snapshot.syncIssue}/>}
        {view.issue && view.issue.code !== "attempt-limit" && <ErrorNotice issue={view.issue}/>}
        {view.invalid && <Notice title="Existing files need manual review" tone="warning">Repair and replacement aren’t available in this version. Figcon won’t overwrite this installation.</Notice>}
        {view.limit && <Notice title="Eight-attempt limit reached" tone="warning" alert>Figcon keeps setup files from every attempt, including successful ones. These files need manual review before setup can continue. Automatic cleanup is not available.</Notice>}
        {snapshot.note && <Notice title="Setup update">{snapshot.note}</Notice>}
        {retention?.state === "requires-review" && !view.limit && <Notice title="Temporary setup files kept" tone="warning">{retention.attemptCount ?? "Some"} setup {retention.attemptCount === 1 ? "attempt is" : "attempts are"} retained for review. This is separate from installation status; Figcon never removes these files automatically.{!!retention.orphanedLedgerFiles && " Extra setup records also need review."}</Notice>}
        {retention?.state === "unknown" && !view.unsupported && <Notice title="Setup files couldn’t be checked" tone="warning">Check status again before starting another installation. Existing files will stay in place.</Notice>}
      </div>
      {view.installed && !view.inProgress ? <div className="next-step"><span className="next-label">UP NEXT</span><h2>Connect your tools.</h2><p>Connecting an AI app and setting up the Figma plugin are separate next steps. That onboarding is coming next; your Figma-to-AI connection isn’t configured yet.</p>{snapshot.syncIssue && <button className="button-text" onClick={onRefresh}>Refresh status</button>}</div> : <div className="actions">
        {snapshot.checking && report && !view.inProgress && <div className="action-status" role="status"><span className="spinner" aria-hidden="true"/>Checking setup…</div>}
        {view.canStart && <button className="button-primary" onClick={onStart}>{view.action}<span aria-hidden="true">↗</span></button>}
        {view.inProgress && <div className="action-status" role="status"><span className="spinner" aria-hidden="true"/>{view.stopping ? "Waiting for cancellation" : "Setup in progress"}</div>}
        {(!view.canStart && !view.inProgress && (snapshot.syncIssue || view.invalid || view.limit || retention?.state === "unknown")) && <button className="button-secondary" onClick={onRefresh}>Check again</button>}
        {view.inProgress && snapshot.syncIssue && <button className="button-text" onClick={onRefresh}>Refresh status</button>}
        {view.canStart && <p className="action-hint">Downloads the essentials into Figcon’s private space.</p>}
      </div>}
    </section>
    <footer className="app-footer"><span>Made for the way you create.</span><span>Figcon · Early access</span></footer>
  </main>;
}
