import type { Component, InstallationError, InstallationEvent, Stage } from "../installer/types.ts";
import { activeOperation, canStart, complete } from "./session.ts";
import type { SetupSnapshot } from "./session.ts";
export const names: Record<Component, string> = { node: "Private runtime", mcp: "Figma Console" };
export const stages: Record<Stage, string> = {
  preparing: "Getting ready", downloading: "Downloading", verifying: "Checking the download", extracting: "Unpacking files", installing: "Installing files", validating: "Verifying setup", activating: "Finishing this step", completed: "Checking the final result", failed: "Checking the result", cancelled: "Confirming cancellation",
};
export function describeError(issue: InstallationError): { title: string; body: string } {
  switch (issue.code) {
    case "unstable-snapshot": return { title: "Refreshing setup…", body: "Setup is changing while we check it. Figcon will keep checking automatically." };
    case "busy": return { title: "Setup is already running", body: "Wait for the other setup to finish, then check again." };
    case "attempt-limit": return { title: "Setup needs a review", body: "Eight setup attempts have been kept. Further installation is paused until those files are reviewed. Automatic cleanup is not available yet." };
    case "cancelled": return { title: "Setup stopped", body: "Completed steps are kept. You can continue when you’re ready." };
    case "integrity-failed": return { title: "A download couldn’t be verified", body: "Check your internet connection and try again. Figcon won’t activate an unverified download." };
    case "deadline-exceeded": return { title: "Setup took too long", body: "Check your internet connection, then try again when you’re ready." };
    case "runtime-required": return { title: "The runtime needs attention", body: "Check setup status before continuing with Figma Console." };
    case "resource-limit": return { title: "Setup reached a resource limit", body: "Check available disk space and review retained setup files before trying again." };
    case "not-owner": return { title: "Use the window that started setup", body: "This installation belongs to another window. You can follow its status here; return there to cancel it." };
    case "already-installed": return { title: "An installation is already there", body: "Check setup status. Figcon will keep existing installations in place." };
    case "unsupported-platform": return { title: "Made for macOS", body: "Managed installation currently supports Apple silicon and Intel Macs. Windows support is coming later." };
    case "validation-failed": return { title: "Setup couldn’t be verified", body: "Check status before retrying. An incomplete active installation needs manual review." };
    default: return { title: "We couldn’t finish this check", body: "Check your connection and setup status, then try again. If this continues, reopen Figcon." };
  }
}
export function downloadLabel(event: InstallationEvent | null): string | null {
  if (!event || event.stage !== "downloading" || event.downloadedBytes === null) return null;
  const mb = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return event.totalBytes === null ? `${mb(event.downloadedBytes)} downloaded` : `${mb(event.downloadedBytes)} of ${mb(event.totalBytes)}`;
}
export function setupView(s: SetupSnapshot) {
  const report = s.report;
  const active = activeOperation(report);
  const installed = complete(report);
  const invalid = !!report && [report.node, report.mcp].some(c => c.diskState === "invalid");
  const limit = !!report && !installed && ((report.retention.attemptCount ?? 0) >= 8 || [report.node, report.mcp].some(c => c.error?.code === "attempt-limit"));
  const unsupported = report?.supported === false;
  const historical = report ? [report.mcp, report.node].find(c => c.lastOperation?.state === "failed" || c.lastOperation?.state === "cancelled" || c.lastOperation?.state === "blocked")?.lastOperation?.error ?? null : null;
  const issue = s.issue ?? (!active && !s.working ? historical : null);
  const observedEvent=active?.lastEvent ?? null;
  const notification=s.progress;
  const event = notification && notification.revision >= (observedEvent?.revision ?? -1) && ((active?.attemptId === notification.attemptId) || (s.working && s.requestedComponent === notification.component && notification.revision > (report?.revision ?? -1))) ? notification : observedEvent;
  const checking = !report;
  const inProgress = !!active || s.working || s.cancelInFlight;
  const stopping = s.cancelInFlight || s.cancelRequested || active?.cancellationRequested === true;
  const title = checking ? "Checking your setup." : unsupported ? "A home on your Mac." : inProgress ? stopping ? "Stopping setup." : "Setting things up." : invalid || limit ? "Setup needs a review." : installed ? "The essentials are ready." : issue?.code === "cancelled" ? "Pick up where you left off." : issue ? "Let’s get you back on track." : "Your next idea starts here.";
  const description = checking ? "Looking for Figcon’s private runtime and Figma Console." : unsupported ? "Figcon’s managed installer is available for macOS. Other platforms can’t install yet." : inProgress ? stopping ? "Waiting for the installer to confirm. Completed steps will stay in place." : "Figcon is downloading and checking the essentials. You can leave this window open." : invalid ? "An existing installation is incomplete or unreadable. Figcon won’t replace it. Manual review is needed before setup can continue." : limit ? "Temporary setup files need review before another installation can begin." : installed ? "Figma Console and its private runtime are installed on this Mac." : "Install Figma Console and its own private Node.js runtime. Both live in Figcon’s space on your Mac.";
  return { report, active, installed, invalid, limit, unsupported, issue, event, checking, inProgress, stopping, title, description,
    canStart: canStart(s),
    canCancel: (!!active || !!s.receipt) && !active?.activated && !stopping && event?.stage !== "activating",
    action: issue ? "Retry setup" : report?.node.diskState === "installed" ? "Continue setup" : "Set up Figma Console",
  };
}
