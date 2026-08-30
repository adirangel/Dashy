import { useCallback, useEffect, useState } from "react";
import type { ProviderId } from "../dashboard";
import {
  getProviderSetupStates,
  installProvider,
  loginProvider,
  type ProviderSetupState,
} from "./api";

export type ProviderSetupController = {
  states: ProviderSetupState[] | null;
  busyProvider: ProviderId | null;
  failureProvider: ProviderId | null;
  loadFailed: boolean;
  reload: () => Promise<void>;
  install: (provider: ProviderId) => Promise<void>;
  login: (provider: ProviderId) => Promise<void>;
};

export function useProviderSetup(): ProviderSetupController {
  const [states, setStates] = useState<ProviderSetupState[] | null>(null);
  const [busyProvider, setBusyProvider] = useState<ProviderId | null>(null);
  const [failureProvider, setFailureProvider] = useState<ProviderId | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);

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
    provider: ProviderId,
    action: (target: ProviderId) => Promise<ProviderSetupState>,
  ) => {
    setBusyProvider(provider);
    setFailureProvider(null);
    try {
      const nextState = await action(provider);
      setStates((current) => current?.map((state) =>
        state.definition.provider === provider ? nextState : state
      ) ?? [nextState]);
    } catch {
      setFailureProvider(provider);
    } finally {
      setBusyProvider(null);
    }
  }, []);

  const install = useCallback(
    (provider: ProviderId) => runAction(provider, installProvider),
    [runAction],
  );
  const login = useCallback(
    (provider: ProviderId) => runAction(provider, loginProvider),
    [runAction],
  );

  return { states, busyProvider, failureProvider, loadFailed, reload, install, login };
}
