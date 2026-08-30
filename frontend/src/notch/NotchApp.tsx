import { useEffect, useReducer, useRef, useState, type AnimationEvent, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { DashboardSnapshot, ProviderId } from "../dashboard";
import { useDashboardSnapshot } from "../useDashboardSnapshot";
import {
  beginNotchExit, completeNotchExit, createExitToken, getCurrentEdgeView, getSettings,
  isTauriRuntime, listenForEdgeView, setNotchInteraction, showNotchMenu,
  type EdgePlacement, type EdgeViewState, type ExitToken, type NotchInteraction,
} from "../window";
import { GitHubCard } from "./GitHubCard";
import { MetricRail } from "./MetricRail";
import { UsageProviderCard } from "./UsageProviderCard";
import "../notch.css";

type NotchAppProps = {
  placement?: EdgePlacement;
  snapshot?: DashboardSnapshot | null;
  selectedProvider?: ProviderId;
  now?: Date;
};

const PROVIDERS: ProviderId[] = ["claude", "codex", "github"];

const isVisibleView = (view: EdgeViewState) =>
  view.visibility === "rail" || view.visibility === "card" || view.visibility === "pinned";

type SurfaceState = {
  authoritative: EdgeViewState;
  rendered: EdgeViewState | null;
  entering: boolean;
  exitRevision: number;
  exit: { token: ExitToken; revision: number; phase: "requesting" | "animating" } | null;
};

type SurfaceAction =
  | { kind: "edgeView"; view: EdgeViewState; exitToken?: ExitToken }
  | { kind: "beginExitResult"; token: ExitToken; revision: number; accepted: boolean }
  | { kind: "entryComplete" }
  | { kind: "exitComplete"; token: ExitToken; revision: number };

function reduceSurface(state: SurfaceState, action: SurfaceAction): SurfaceState {
  if (action.kind === "entryComplete") return { ...state, entering: false };
  if (action.kind === "beginExitResult") {
    if (!state.exit || state.exit.token !== action.token || state.exit.revision !== action.revision) return state;
    if (!action.accepted) return { ...state, rendered: null, exit: null };
    return { ...state, exit: { ...state.exit, phase: "animating" } };
  }
  if (action.kind === "exitComplete") {
    if (!state.exit || state.exit.token !== action.token || state.exit.revision !== action.revision) return state;
    return { ...state, rendered: null, exit: null };
  }
  const next = action.view;
  if (isVisibleView(next)) {
    return {
      authoritative: next,
      rendered: next,
      entering: state.rendered === null || state.exit !== null,
      exitRevision: state.exitRevision,
      exit: null,
    };
  }
  if (state.rendered && !state.exit && action.exitToken) {
    const revision = state.exitRevision + 1;
    return {
      authoritative: next,
      rendered: state.rendered,
      entering: false,
      exitRevision: revision,
      exit: { token: action.exitToken, revision, phase: "requesting" },
    };
  }
  return { ...state, authoritative: next };
}

export function NotchApp({
  placement: placementProp,
  snapshot: snapshotProp,
  selectedProvider: selectedProp,
  now = new Date(),
}: NotchAppProps) {
  const { t } = useTranslation();
  const dashboard = useDashboardSnapshot();
  const native = isTauriRuntime();
  const [persistedPlacement, setPersistedPlacement] = useState<EdgePlacement>("right");
  const [selectedProvider, setSelectedProvider] = useState<ProviderId>(selectedProp ?? "claude");
  const initialEdgeView: EdgeViewState = {
    visibility: native ? "hidden" : "card",
    placement: placementProp ?? "right",
    provider: selectedProp ?? "claude",
  };
  const [surface, dispatchSurface] = useReducer(reduceSurface, {
    authoritative: initialEdgeView,
    rendered: native ? null : initialEdgeView,
    entering: false,
    exitRevision: 0,
    exit: null,
  });
  const edgeView = surface.authoritative;
  const surfaceRef = useRef<HTMLElement>(null);
  const lastSelected = useRef<ProviderId>(selectedProp ?? "claude");
  const edgeViewRef = useRef(edgeView);
  edgeViewRef.current = edgeView;
  const focusRestoreArmed = useRef<ProviderId | null>(null);
  const suppressFocusSelection = useRef<ProviderId | null>(null);
  const acknowledgedExit = useRef<ExitToken | null>(null);

  const focusMetric = (provider: ProviderId) => {
    queueMicrotask(() => {
      const button = surfaceRef.current?.querySelector<HTMLButtonElement>(`[data-provider="${provider}"]`);
      if (button && document.activeElement !== button) {
        suppressFocusSelection.current = provider;
        button.focus();
        queueMicrotask(() => {
          if (suppressFocusSelection.current === provider) suppressFocusSelection.current = null;
        });
      }
    });
  };

  useEffect(() => {
    if (placementProp || !native) return;
    let active = true;
    void getSettings().then((settings) => {
      if (active) setPersistedPlacement(settings.placement);
    }).catch(() => undefined);
    return () => { active = false; };
  }, [native, placementProp]);

  useEffect(() => {
    if (!native) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    let eventRevision = 0;
    const applyView = (next: EdgeViewState) => {
      if (!isVisibleView(next)) focusRestoreArmed.current = null;
      if (next.provider) {
        lastSelected.current = next.provider;
        setSelectedProvider(next.provider);
      }
      dispatchSurface({
        kind: "edgeView",
        view: next,
        exitToken: isVisibleView(next) ? undefined : createExitToken(),
      });
    };
    void listenForEdgeView((next) => {
      if (!active) return;
      eventRevision += 1;
      applyView(next);
    }).then(async (stop) => {
      if (!active) {
        stop();
        return;
      }
      unlisten = stop;
      const queryRevision = eventRevision;
      try {
        const current = await getCurrentEdgeView();
        if (active && eventRevision === queryRevision) applyView(current);
      } catch {
        // A later edge event remains authoritative when the handshake is unavailable.
      }
    }).catch(() => undefined);
    return () => { active = false; unlisten?.(); };
  }, [native]);

  useEffect(() => {
    if (selectedProp) {
      lastSelected.current = selectedProp;
      setSelectedProvider(selectedProp);
    }
  }, [selectedProp]);

  useEffect(() => {
    const armedProvider = focusRestoreArmed.current;
    if (!armedProvider || !isVisibleView(edgeView)) return;
    focusMetric(edgeView.provider ?? armedProvider);
    focusRestoreArmed.current = null;
  }, [edgeView]);

  useEffect(() => {
    if (!native) return;
    const onNativeFocus = () => {
      const current = edgeViewRef.current;
      if (!isVisibleView(current)) return;
      const provider = current.provider ?? lastSelected.current;
      focusRestoreArmed.current = provider;
      focusMetric(provider);
      focusRestoreArmed.current = null;
    };
    const onNativeBlur = () => {
      const current = edgeViewRef.current;
      if (current.visibility === "pinned") {
        focusRestoreArmed.current = current.provider ?? lastSelected.current;
      }
    };
    window.addEventListener("focus", onNativeFocus);
    window.addEventListener("blur", onNativeBlur);
    return () => {
      window.removeEventListener("focus", onNativeFocus);
      window.removeEventListener("blur", onNativeBlur);
    };
  }, [native]);

  const placement = placementProp ?? (native ? (surface.rendered?.placement ?? edgeView.placement) : persistedPlacement);
  const snapshot = snapshotProp !== undefined ? snapshotProp : dashboard.snapshot;
  const selectedIsStale = snapshot?.[selectedProvider].status === "stale";
  const visible = surface.rendered !== null;
  const showCard = surface.rendered?.visibility === "card" || surface.rendered?.visibility === "pinned";
  const logicalSize = showCard
    ? placement === "top" ? "340x430" : "370x360"
    : placement === "top" ? "270x70" : "70x270";
  const send = (interaction: NotchInteraction) => {
    if (native) void setNotchInteraction(interaction).catch(() => undefined);
  };
  const select = (provider: ProviderId) => {
    if (edgeView.visibility === "pinned" || surface.exit) return;
    lastSelected.current = provider;
    setSelectedProvider(provider);
    send({ kind: "selectProvider", provider });
  };
  const activate = (provider: ProviderId) => {
    if (surface.exit) return;
    const isUnpinningCurrent = edgeView.visibility === "pinned" && edgeView.provider === provider;
    if (isUnpinningCurrent) focusRestoreArmed.current = provider;
    if (!native) {
      lastSelected.current = provider;
      setSelectedProvider(provider);
    }
    send({ kind: "togglePin", provider });
    if (!isUnpinningCurrent) void dashboard.refreshProvider(provider);
  };
  const selectFromFocus = (provider: ProviderId) => {
    if (suppressFocusSelection.current === provider) {
      suppressFocusSelection.current = null;
      return;
    }
    select(provider);
  };
  const focusProvider = (provider: ProviderId) => {
    if (edgeView.visibility !== "pinned") select(provider);
    const button = surfaceRef.current?.querySelector<HTMLButtonElement>(`[data-provider="${provider}"]`);
    if (button && document.activeElement !== button) {
      suppressFocusSelection.current = provider;
      button.focus();
      suppressFocusSelection.current = null;
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (surface.exit) return;
    if (event.key === "Escape") {
      event.preventDefault();
      focusRestoreArmed.current = edgeView.visibility === "card" || edgeView.visibility === "pinned"
        ? edgeView.provider ?? lastSelected.current
        : null;
      send({ kind: "escape" });
      return;
    }
    const forward = placement === "top" ? event.key === "ArrowRight" : event.key === "ArrowDown";
    const backward = placement === "top" ? event.key === "ArrowLeft" : event.key === "ArrowUp";
    if (forward || backward) {
      event.preventDefault();
      const active = (document.activeElement as HTMLElement | null)?.dataset.provider as ProviderId | undefined;
      const currentIndex = PROVIDERS.indexOf(active ?? selectedProvider);
      const delta = forward ? 1 : -1;
      focusProvider(PROVIDERS[(currentIndex + delta + PROVIDERS.length) % PROVIDERS.length]);
      return;
    }
    if (edgeView.visibility === "pinned" && event.key === "Tab") {
      const focusable = Array.from(surfaceRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? []);
      if (focusable.length === 0) return;
      const activeIndex = focusable.indexOf(document.activeElement as HTMLElement);
      const nextIndex = event.shiftKey
        ? (activeIndex <= 0 ? focusable.length - 1 : activeIndex - 1)
        : (activeIndex < 0 || activeIndex === focusable.length - 1 ? 0 : activeIndex + 1);
      event.preventDefault();
      focusable[nextIndex].focus();
    }
  };

  useEffect(() => {
    const exit = surface.exit;
    if (!native || !exit || exit.phase !== "requesting") return;
    let active = true;
    void beginNotchExit(exit.token).then((accepted) => {
      if (active) dispatchSurface({
        kind: "beginExitResult", token: exit.token, revision: exit.revision, accepted,
      });
    }).catch(() => {
      if (active) dispatchSurface({
        kind: "beginExitResult", token: exit.token, revision: exit.revision, accepted: false,
      });
    });
    return () => { active = false; };
  }, [native, surface.exit]);

  const onSurfaceAnimationEnd = (event: AnimationEvent<HTMLElement>) => {
    if (event.target !== event.currentTarget) return;
    const exit = surface.exit;
    if (exit?.phase === "animating") {
      if (acknowledgedExit.current === exit.token) return;
      acknowledgedExit.current = exit.token;
      if (native) void completeNotchExit(exit.token).catch(() => undefined);
      dispatchSurface({ kind: "exitComplete", token: exit.token, revision: exit.revision });
    } else if (surface.entering) {
      dispatchSurface({ kind: "entryComplete" });
    }
  };

  return <main className="notch-stage" data-testid="notch-app" data-visibility={edgeView.visibility}>
    {visible && <section
      key={surface.exit?.token ?? `visible-${surface.exitRevision}`}
      ref={surfaceRef}
      className={`notch-surface placement-${placement}${showCard ? " is-expanded" : ""}${surface.entering ? " is-entering" : ""}${surface.exit?.phase === "requesting" ? " is-exit-pending" : ""}${surface.exit?.phase === "animating" ? " is-exiting" : ""}`}
      data-testid="notch-surface"
      data-placement={placement}
      data-logical-size={logicalSize}
      aria-hidden={surface.exit !== null || undefined}
      onAnimationEnd={onSurfaceAnimationEnd}
      onPointerEnter={() => { if (!surface.exit) send({ kind: "enterSafeRegion" }); }}
      onPointerLeave={() => { if (!surface.exit) send({ kind: "leaveSafeRegion" }); }}
      onKeyDown={onKeyDown}
      onContextMenu={(event) => {
        event.preventDefault();
        if (native) void showNotchMenu().catch(() => undefined);
      }}
    >
      <div className="notch-content" inert={surface.exit !== null}>
      <MetricRail
        placement={placement}
        snapshot={snapshot}
        selectedProvider={selectedProvider}
        onSelect={select}
        onFocusSelect={selectFromFocus}
        onActivate={activate}
        refreshingProviders={dashboard.refreshingProviders}
        now={now}
      />
      {showCard && <div className="notch-join" data-provider={selectedProvider} aria-hidden="true" />}
      {showCard && <div className="provider-card-slot" data-testid="provider-card-region">
        {selectedProvider === "github"
          ? <GitHubCard snapshot={snapshot?.github ?? null} now={now} />
          : <UsageProviderCard provider={selectedProvider} snapshot={snapshot?.[selectedProvider] ?? null} />}
      </div>}
      <div className="notch-live-region" role="status" aria-live="polite" aria-atomic="true">
        {selectedIsStale || dashboard.refreshFailures.has(selectedProvider)
          ? `${t(`providers.${selectedProvider}`)}: ${t("status.stale")}`
          : ""}
      </div>
      </div>
    </section>}
  </main>;
}
