import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { SupportedLocale } from "./i18n";
import type { ProviderId } from "./dashboard";

export type EdgePlacement = "right" | "left" | "top";
type StoredMonitorRect = { x: number; y: number; width: number; height: number };
type MonitorPreference = { id: string; name: string; lastWorkArea: StoredMonitorRect };
export type AppSettings = {
  placement: EdgePlacement;
  monitor: MonitorPreference | null;
  locale: SupportedLocale;
  alwaysShowOverFullscreen: boolean;
  onboardingCompleted: boolean;
  enabledProviders: ProviderId[];
  providerSetupVersion?: number;
};
export type SettingsPatch = Partial<Omit<AppSettings, "providerSetupVersion">>;
export type MonitorInfo = StoredMonitorRect & {
  id: string;
  name: string;
  primary: boolean;
};
export type TrayLabels = {
  show: string;
  refreshAll: string;
  placement: string;
  right: string;
  left: string;
  top: string;
  monitor: string;
  primaryMonitor: string;
  unavailable: string;
  settings: string;
  quit: string;
};
type EdgeVisibility = "hidden" | "rail" | "card" | "pinned" | "suppressed";
export type EdgeViewState = {
  visibility: EdgeVisibility;
  placement: EdgePlacement;
  provider: ProviderId | null;
};
export type ExitToken = string;
type DashboardCacheChangedEvent = { revision: number };
export type NotchInteraction =
  | { kind: "enterSafeRegion" }
  | { kind: "leaveSafeRegion" }
  | { kind: "selectProvider"; provider: ProviderId }
  | { kind: "clearProvider" }
  | { kind: "togglePin"; provider: ProviderId }
  | { kind: "outsideClick" }
  | { kind: "escape" };

const LOCALE_CHANGED_EVENT = "dashy://locale-changed";
const SETTINGS_CHANGED_EVENT = "dashy://settings-changed";
const EDGE_VIEW_EVENT = "dashy://edge-view";
const DASHBOARD_CACHE_CHANGED_EVENT = "dashy://dashboard-cache-changed";
const placements = new Set<EdgePlacement>(["right", "left", "top"]);
const visibilities = new Set<EdgeVisibility>(["hidden", "rail", "card", "pinned", "suppressed"]);
const providers = new Set<ProviderId>(["claude", "codex", "github"]);
const EXIT_TOKEN_PATTERN = /^[a-z0-9-]{1,32}$/;
let exitTokenSequence = 0;

export function isDashboardCacheChangedEvent(value: unknown): value is DashboardCacheChangedEvent {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return Object.keys(candidate).length === 1
    && Number.isInteger(candidate.revision)
    && (candidate.revision as number) >= 1
    && (candidate.revision as number) <= 0xffff_ffff;
}

export function isExitToken(value: unknown): value is ExitToken {
  return typeof value === "string" && EXIT_TOKEN_PATTERN.test(value);
}

export function createExitToken(): ExitToken {
  exitTokenSequence = exitTokenSequence >= Number.MAX_SAFE_INTEGER ? 1 : exitTokenSequence + 1;
  const token = `exit-${Math.max(0, Date.now()).toString(36)}-${exitTokenSequence.toString(36)}`;
  if (!isExitToken(token)) throw new Error("invalid exit token");
  return token;
}

export function isEdgeViewState(value: unknown): value is EdgeViewState {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  const keys = Object.keys(candidate);
  if (!(keys.length === 3
    && keys.every((key) => key === "visibility" || key === "placement" || key === "provider")
    && visibilities.has(candidate.visibility as EdgeVisibility)
    && placements.has(candidate.placement as EdgePlacement)
    && (candidate.provider === null || providers.has(candidate.provider as ProviderId)))) return false;
  const needsProvider = candidate.visibility === "card" || candidate.visibility === "pinned";
  return needsProvider ? candidate.provider !== null : candidate.provider === null;
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function currentWindowLabel(): string {
  if (!isTauriRuntime()) return "main";
  try {
    return getCurrentWindow().label;
  } catch {
    return "main";
  }
}

export const getSettings = () => invoke<AppSettings>("get_settings");
export const updateSettings = (patch: SettingsPatch) =>
  invoke<AppSettings>("update_settings", { patch });
export const completeOnboarding = (enabledProviders: ProviderId[], locale: SupportedLocale) =>
  invoke<AppSettings>("complete_onboarding", { enabledProviders, locale });
export const listMonitors = () => invoke<MonitorInfo[]>("list_monitors");
export const setTrayLabels = (labels: TrayLabels) =>
  invoke<void>("set_tray_labels", { labels });
export const setNotchInteraction = (interaction: NotchInteraction) =>
  invoke<void>("set_notch_interaction", { interaction });
export const showNotchMenu = () => invoke<void>("show_notch_menu");
export const openSettings = () => invoke<void>("open_settings");
export function beginNotchExit(token: ExitToken): Promise<boolean> {
  if (!isExitToken(token)) return Promise.reject(new Error("invalid exit token"));
  return invoke<boolean>("begin_notch_exit", { request: { token } });
}
export function completeNotchExit(token: ExitToken): Promise<boolean> {
  if (!isExitToken(token)) return Promise.reject(new Error("invalid exit token"));
  return invoke<boolean>("complete_notch_exit", { request: { token } });
}
export async function getCurrentEdgeView(): Promise<EdgeViewState> {
  const value = await invoke<unknown>("get_current_edge_view");
  if (!isEdgeViewState(value)) throw new Error("invalid edge view response");
  return value;
}

export async function listenForEdgeView(
  handler: (view: EdgeViewState) => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => undefined;
  return listen<unknown>(EDGE_VIEW_EVENT, ({ payload }) => {
    if (isEdgeViewState(payload)) handler(payload);
  });
}

export async function listenForDashboardCacheChanged(
  handler: (event: DashboardCacheChangedEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => undefined;
  return listen<unknown>(DASHBOARD_CACHE_CHANGED_EVENT, ({ payload }) => {
    if (isDashboardCacheChangedEvent(payload)) handler(payload);
  });
}

export async function isCurrentWindowActive(): Promise<boolean> {
  if (!isTauriRuntime()) return true;
  const currentWindow = getCurrentWindow();
  const signals = await Promise.allSettled([
    currentWindow.isVisible(),
    currentWindow.isFocused(),
  ]);
  return signals.some((signal) => signal.status === "fulfilled" && signal.value);
}

export async function listenForCurrentWindowActivation(
  handler: () => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => undefined;
  return getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (focused) handler();
  });
}

export async function listenForSettingsChanges(
  handler: (settings: AppSettings) => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => undefined;
  return listen<AppSettings>(SETTINGS_CHANGED_EVENT, ({ payload }) => handler(payload));
}

export async function emitLocaleChanged(locale: SupportedLocale): Promise<void> {
  if (!isTauriRuntime()) return;
  await emitTo("main", LOCALE_CHANGED_EVENT, { locale });
}

export async function listenForLocaleChanges(
  handler: (locale: unknown) => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => undefined;
  return listen<{ locale?: unknown }>(LOCALE_CHANGED_EVENT, ({ payload }) => {
    handler(payload?.locale);
  });
}
