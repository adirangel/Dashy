import { invoke } from "@tauri-apps/api/core";
import type { ProviderId, ProviderStatus } from "../dashboard";

export type ProviderInstallKind = "winget" | "manualUrl";

export type ProviderSetupDefinition = {
  provider: ProviderId;
  publisher: string;
  packageId: string | null;
  installKind: ProviderInstallKind;
  installCommand: string;
  installUrl: string;
  loginCommand: string;
};

export type ProviderSetupState = {
  definition: ProviderSetupDefinition;
  status: ProviderStatus;
  repairAction: ProviderSetupAction | null;
};

export type ProviderSetupAction = "install" | "login";

const providerIds = new Set<ProviderId>(["claude", "codex", "github", "grok", "cursor"]);
const providerStatuses = new Set<ProviderStatus>([
  "connected", "stale", "notInstalled", "notAuthenticated", "unavailable",
]);
const approvedInstallUrls: Record<ProviderId, string> = {
  claude: "https://code.claude.com/docs/en/setup",
  codex: "https://learn.chatgpt.com/docs/codex/cli",
  github: "https://cli.github.com/",
  grok: "https://docs.x.ai/build/overview",
  cursor: "https://cursor.com/docs/cli/installation",
};

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const valueKeys = Object.keys(value);
  return valueKeys.length === keys.length && valueKeys.every((key) => keys.includes(key));
}

function isProviderSetupDefinition(value: unknown): value is ProviderSetupDefinition {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return hasExactKeys(candidate, [
    "provider", "publisher", "packageId", "installKind", "installCommand", "installUrl",
    "loginCommand",
  ])
    && providerIds.has(candidate.provider as ProviderId)
    && typeof candidate.publisher === "string"
    && (candidate.packageId === null || typeof candidate.packageId === "string")
    && (candidate.installKind === "winget" || candidate.installKind === "manualUrl")
    // A winget install needs a package id; a manual install must not claim one.
    && (candidate.installKind === "winget") === (typeof candidate.packageId === "string")
    && typeof candidate.installCommand === "string"
    && candidate.installUrl === approvedInstallUrls[candidate.provider as ProviderId]
    && typeof candidate.loginCommand === "string";
}

function isProviderSetupState(value: unknown): value is ProviderSetupState {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return hasExactKeys(candidate, ["definition", "status", "repairAction"])
    && isProviderSetupDefinition(candidate.definition)
    && providerStatuses.has(candidate.status as ProviderStatus)
    && (candidate.repairAction === null
      || candidate.repairAction === "install"
      || candidate.repairAction === "login");
}

function providerSetupResponseError(): Error {
  return new Error("invalid provider setup response");
}

export async function getProviderSetupStates(): Promise<ProviderSetupState[]> {
  const response = await invoke<unknown>("get_provider_setup_states");
  if (!Array.isArray(response) || !response.every(isProviderSetupState)) {
    throw providerSetupResponseError();
  }
  return response;
}

async function runProviderSetupAction(
  action: ProviderSetupAction,
  provider: ProviderId,
): Promise<ProviderSetupState> {
  const response = await invoke<unknown>(`${action}_provider`, { request: { provider } });
  if (!isProviderSetupState(response) || response.definition.provider !== provider) {
    throw providerSetupResponseError();
  }
  return response;
}

export function installProvider(provider: ProviderId): Promise<ProviderSetupState> {
  return runProviderSetupAction("install", provider);
}

export function loginProvider(provider: ProviderId): Promise<ProviderSetupState> {
  return runProviderSetupAction("login", provider);
}
