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

export function useProviderSetup(): ProviderSetupController {
  const [states, setStates] = useState<ProviderSetupState[] | null>(null);
  const [busyProvider, setBusyProvider] = useState<ProviderId | null>(null);
  const [busyAction, setBusyAction] = useState<ProviderSetupAction | null>(null);
  const [failureProvider, setFailureProvider] = useState<ProviderId | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const actionInFlight = useRef(false);

  const reload = useCallback(async () => {
    try {
      const nextStates = await getProviderSetupStates();
      setStates(nextStates);
      setLoadFailed(false);
    } catch {
      setLoadFailed(true);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const runAction = useCallback(async (
    actionName: ProviderSetupAction,
    provider: ProviderId,
    action: (target: ProviderId) => Promise<ProviderSetupState>,
  ) => {
    if (actionInFlight.current) return;
    actionInFlight.current = true;
    setBusyProvider(provider);
    setBusyAction(actionName);
    setFailureProvider(null);
    try {
      const nextState = await action(provider);
      setStates((current) => current?.map((state) =>
        state.definition.provider === provider ? nextState : state
      ) ?? [nextState]);
    } catch {
      setFailureProvider(provider);
    } finally {
      actionInFlight.current = false;
      setBusyProvider(null);
      setBusyAction(null);
    }
  }, []);

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
