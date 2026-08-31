import { useEffect, useState } from "react";
import {
  isCurrentWindowActive,
  isTauriRuntime,
  listenForCurrentWindowActivation,
} from "./window";

export function useWindowActivationRevision(): number {
  const nativeRuntime = isTauriRuntime();
  const [revision, setRevision] = useState(nativeRuntime ? 0 : 1);

  useEffect(() => {
    if (!nativeRuntime) return;
    let mounted = true;
    let unlisten: (() => void) | undefined;
    let focusRevision = 0;
    const activate = () => {
      focusRevision += 1;
      if (mounted) setRevision((current) => current + 1);
    };

    const installListenerThenQuery = async () => {
      const startingFocusRevision = focusRevision;
      try {
        unlisten = await listenForCurrentWindowActivation(activate);
      } catch {
        // The visibility query still gives an already-open window one safe activation chance.
      }
      if (!mounted) {
        unlisten?.();
        return;
      }
      try {
        const active = await isCurrentWindowActive();
        if (mounted && active && focusRevision === startingFocusRevision) activate();
      } catch {
        // A later native focus event can still activate the window.
      }
    };

    void installListenerThenQuery();
    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [nativeRuntime]);

  return revision;
}
