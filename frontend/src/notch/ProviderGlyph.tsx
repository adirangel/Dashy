import { siClaude, siCursor, siGithub, siOpenaigym } from "simple-icons";
import type { ProviderId } from "../dashboard";

type GlyphIcon = { path: string; viewBox?: string };

// Grok has no simple-icons entry (checked through 16.29.0); this is the official
// mark from grok.com's favicon, kept in its source 512-unit coordinate space.
const grokIcon: GlyphIcon = {
  viewBox: "0 0 512 512",
  path:
    "M210.484 312.759L343.465 210.383C349.984 205.364 359.302 207.322 362.408 " +
    "215.117C378.758 256.231 371.454 305.64 338.925 339.563C306.397 373.487 " +
    "261.137 380.927 219.768 363.983L174.577 385.803C239.394 432.008 318.104 " +
    "420.581 367.289 369.251C406.303 328.564 418.386 273.104 407.088 223.091L407.19 " +
    "223.198C390.807 149.726 411.218 120.359 453.03 60.3072C454.02 58.8833 455.01 " +
    "57.4595 456 56L400.978 113.382V113.204L210.45 312.794Z " +
    "M183.042 337.641C136.519 291.294 144.54 219.567 184.236 178.203C213.59 147.59 " +
    "261.683 135.096 303.666 153.464L348.755 131.75C340.632 125.627 330.221 119.042 " +
    "318.275 114.414C264.277 91.2407 199.63 102.774 155.735 148.516C113.513 192.549 " +
    "100.236 260.254 123.036 318.027C140.069 361.206 112.148 391.748 84.0229 " +
    "422.575C74.0561 433.503 64.0553 444.431 56 456L183.007 337.677Z",
};

const PROVIDER_ICONS: Record<ProviderId, GlyphIcon> = {
  claude: siClaude,
  codex: siOpenaigym,
  github: siGithub,
  grok: grokIcon,
  cursor: siCursor,
};

export function ProviderGlyph({ provider }: { provider: ProviderId }) {
  const icon = PROVIDER_ICONS[provider];
  return <svg
    aria-hidden="true"
    className="provider-glyph"
    data-testid={`provider-glyph-${provider}`}
    viewBox={icon.viewBox ?? "0 0 24 24"}
    focusable="false"
  >
    <path d={icon.path} fill="currentColor" />
  </svg>;
}
