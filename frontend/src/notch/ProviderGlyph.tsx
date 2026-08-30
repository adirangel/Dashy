import { siClaude, siGithub, siOpenaigym, type SimpleIcon } from "simple-icons";
import type { ProviderId } from "../dashboard";

const PROVIDER_ICONS: Record<ProviderId, SimpleIcon> = {
  claude: siClaude,
  codex: siOpenaigym,
  github: siGithub,
};

export function ProviderGlyph({ provider }: { provider: ProviderId }) {
  const icon = PROVIDER_ICONS[provider];
  return <svg
    aria-hidden="true"
    className="provider-glyph"
    data-testid={`provider-glyph-${provider}`}
    viewBox="0 0 24 24"
    focusable="false"
  >
    <path d={icon.path} fill="currentColor" />
  </svg>;
}
