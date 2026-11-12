import type { ClientInspection } from "../client-integration/types.ts";
import type { Issue } from "./session.ts";
export const clientNames = { "claude-code": "Claude Code", "codex-cli": "Codex" };
export function clientStatus(client: ClientInspection): { title: string; body: string; tone: "neutral" | "warning" } {
  if (client.detection === "missing") return { title: "Not detected", body: "Figcon couldn’t find this client. It won’t install or update it for you.", tone: "neutral" };
  if (client.detection === "ambiguous") return { title: "More than one installation", body: "Choose which installation to use before connecting. Automatic selection isn’t available yet.", tone: "warning" };
  if (client.detection === "unsupported-platform") return { title: "Unavailable on this platform", body: "Client setup currently supports macOS.", tone: "warning" };
  if (!["missing", "readable"].includes(client.configuration)) return { title: "Settings need review", body: client.configuration === "malformed" ? "The settings file couldn’t be understood. Figcon won’t repair or replace it." : "These settings are unsupported, unsafe or unavailable. Figcon will leave them in place.", tone: "warning" };
  if (client.registration === "matching-external" || client.registration === "external") return { title: "Configured outside Figcon", body: "A Figma Console registration already exists. Figcon won’t adopt, replace or remove it. A live connection hasn’t been checked.", tone: "neutral" };
  if (client.registration === "conflict" || client.registration === "unsupported") return { title: "Existing registration needs review", body: "The proposed name or settings are already in use. Figcon won’t overwrite them.", tone: "warning" };
  if (client.registration === "unknown") return { title: "Registration couldn’t be checked", body: "Refresh to inspect again. No changes will be made.", tone: "warning" };
  return { title: "Not configured", body: "Review a proposal for using Figcon’s private Figma Console installation.", tone: "neutral" };
}
export const warnings: Record<string, string> = {
  "production-disabled": "Client configuration changes are disabled in this version.",
  "external-writer-race": "Other apps can change their settings while Figcon is checking them. Changed settings require a new review.",
  "backup-required-for-existing-file": "A future enabled change would keep an exact private backup of existing settings, which may include sensitive information.",
  "recovery-files-retained": "Recovery files are kept for manual review. Figcon never deletes them automatically.",
  "effective-configuration-unknown": "Project or organisation settings may override this user-wide registration. A registration alone does not confirm a Figma connection.",
};
export function describeIssue(issue: Issue): { title: string; body: string } {
  switch (issue) {
    case "activation-disabled": return { title: "Changes are blocked in this version", body: "Your approval was not applied. The backend safety gate remains closed, and no client connection was confirmed." };
    case "stale-plan": return { title: "Settings changed since this preview", body: "Refresh and review a new plan before approving again. Figcon won’t reuse this approval." };
    case "expired-plan": case "unknown-plan": return { title: "This plan has expired", body: "Request a new preview. Your earlier confirmation can’t be reused." };
    case "consumed-plan": return { title: "This plan has already been used", body: "Refresh before requesting a new preview. Figcon won’t submit it again." };
    case "prerequisites-missing": return { title: "Finish setup first", body: "The private runtime and Figma Console must both be installed and verified." };
    case "client-unavailable": return { title: "Client unavailable", body: "Refresh the client list. The installation may be missing or ambiguous." };
    case "unsupported-configuration": case "conflict": return { title: "Client settings need review", body: "Existing or unsupported settings block this proposal. Figcon won’t replace them." };
    case "registry-capacity": return { title: "Too many pending plans", body: "Wait a few minutes for older plans to expire, then refresh." };
    case "recovery-capacity": return { title: "Recovery files need manual review", body: "The storage allowance is full or could not be checked. No files will be deleted automatically." };
    case "not-owner": return { title: "Return to the original window", body: "Only the window that prepared this plan can approve or decline it." };
    case "uncertain": return { title: "The outcome needs manual review", body: "A change may have been applied. Refresh to inspect the current registration. Recovery files must be preserved; do not retry blindly." };
    case "unsupported-platform": return { title: "Available on macOS", body: "Client configuration previews aren’t supported on this platform yet." };
    case "rejected": return { title: "Plan declined", body: "This plan can no longer be approved." };
    case "unexpected-response": return { title: "The response needs review", body: "Figcon received a response it can’t safely use. No connection is confirmed. Refresh to inspect the current settings." };
    default: return { title: "We couldn’t complete this check", body: "Refresh and try again. Figcon won’t reuse this plan or show private error details." };
  }
}
