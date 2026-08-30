import { useCallback, useEffect, useRef, useState } from "react";
import type { ProviderId } from "../dashboard";
import {
  getProviderSetupStates,
  installProvider,
  loginProvider,
  type ProviderSetupAction,
  type ProviderSetupState,
} from "./api";

export type ProviderSetupController = {
  states: ProviderSetupState[] | null;
  busyProvider: ProviderId | null;
  busyAction: ProviderSetupAction | null;
  failureProvider: ProviderId | null;
  loadFailed: boolean;
  reload: () => Promise<void>;
  install: (provider: ProviderId) => Promise<void>;
  login: (provider: ProviderId) => Promise<void>;
};

export function useProviderSetup(activationRevision = 1): ProviderSetupController {
  const [states, setStates] = useState<ProviderSetupState[] | null>(null);
  const [busyProvider, setBusyProvider] = useState<ProviderId | null>(null);
  const [busyAction, setBusyAction] = useState<ProviderSetupAction | null>(null);
  const [failureProvider, setFailureProvider] = useState<ProviderId | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const actionInFlight = useRef(false);
  const mounted = useRef(true);
  const loadRequest = useRef(0);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      loadRequest.current += 1;
    };
  }, []);

  const reload = useCallback(async () => {
    const request = ++loadRequest.current;
    try {
      const nextStates = await getProviderSetupStates();
      if (!mounted.current || request !== loadRequest.current) return;
      setStates(nextStates);
      setLoadFailed(false);
    } catch {
      if (mounted.current && request === loadRequest.current) setLoadFailed(true);
    }
  }, []);

  useEffect(() => {
    if (activationRevision <= 0) return;
    void reload();
  }, [activationRevision, reload]);

  const runAction = useCallback(async (
    actionName: ProviderSetupAction,
    provider: ProviderId,
    action: (target: ProviderId) => Promise<ProviderSetupState>,
  ) => {
    if (actionInFlight.current) return;
    actionInFlight.current = true;
    loadRequest.current += 1;
    setBusyProvider(provider);
    setBusyAction(actionName);
    setFailureProvider(null);
    try {
      const nextState = await action(provider);
      if (mounted.current) {
        setStates((current) => current?.map((state) =>
          state.definition.provider === provider ? nextState : state
        ) ?? [nextState]);
      }
    } catch {
      if (mounted.current) setFailureProvider(provider);
      await reload();
    } finally {
      actionInFlight.current = false;
      if (mounted.current) {
        setBusyProvider(null);
        setBusyAction(null);
      }
    }
  }, [reload]);

  const install = useCallback(
    (provider: ProviderId) => runAction("install", provider, installProvider),
    [runAction],
  );
  const login = useCallback(
    (provider: ProviderId) => runAction("login", provider, loginProvider),
    [runAction],
  );

  return { states, busyProvider, busyAction, failureProvider, loadFailed, reload, install, login };
}
