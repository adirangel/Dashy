export type Messages = {
  providers: { claude: string; codex: string; github: string; grok: string; cursor: string };
  usage: { shortWindow: string; weeklyWindow: string; monthlyWindow: string; remaining: string; resets: string };
  github: { streakDays: string; today: string; contributions: string; heatmapLabel: string };
  cursor: { plan: string; account: string; usageHint: string };
  status: { loading: string; notInstalled: string; signInRequired: string; unavailable: string; stale: string; lastUpdated: string };
  guidance: { installClaude: string; installCodex: string; installGitHub: string; installGrok: string; installCursor: string; signInClaude: string; signInCodex: string; signInGitHub: string; signInGrok: string; signInCursor: string; retryLater: string };
  setup: {
    eyebrow: string; title: string; description: string; useProvider: string;
    languageTitle: string; languageDescription: string; continue: string; back: string; stepLabel: string;
    connected: string; notInstalled: string; signInRequired: string; needsAttention: string;
    installing: string; connecting: string;
    install: string; connect: string; retry: string; cancel: string;
    confirmInstall: string; confirmLogin: string; installDisclosure: string; installManualDisclosure: string; loginDisclosure: string;
    publisher: string; packageId: string; command: string; manualHelp: string; manualHelpFailure: string;
    finish: string; finishFailure: string; actionFailure: string; loading: string;
  };
  settings: { title: string; placement: string; right: string; left: string; top: string; monitor: string; language: string; fullscreen: string; startup: string; providerStatus: string };
  menu: { show: string; refreshAll: string; placement: string; monitor: string; primaryMonitor: string; settings: string; quit: string };
  actions: { refresh: string; refreshAll: string; openSettings: string; close: string };
};

const en = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub", grok: "Grok", cursor: "Cursor" },
  usage: { shortWindow: "Current session", weeklyWindow: "Weekly", monthlyWindow: "Monthly", remaining: "{{value}}% remaining", resets: "Resets {{time}}" },
  github: { streakDays: "{{count}} day streak", today: "Today", contributions: "{{count}} contributions", heatmapLabel: "GitHub contributions over the last 12 weeks" },
  cursor: { plan: "Plan", account: "Account", usageHint: "Cursor does not report usage limits. See usage on the Cursor dashboard." },
  status: { loading: "Loading", notInstalled: "Not installed", signInRequired: "Sign in required", unavailable: "Unavailable", stale: "Last known data", lastUpdated: "Last updated {{time}}" },
  guidance: { installClaude: "Install the Claude CLI, then reopen Dashy.", installCodex: "Install the Codex CLI, then reopen Dashy.", installGitHub: "Install the GitHub CLI, then reopen Dashy.", installGrok: "Install the Grok CLI, then reopen Dashy.", installCursor: "Install the Cursor CLI, then reopen Dashy.", signInClaude: "Sign in to Claude, then retry.", signInCodex: "Sign in to Codex, then retry.", signInGitHub: "Sign in to GitHub, then retry.", signInGrok: "Sign in to Grok, then retry.", signInCursor: "Sign in to Cursor, then retry.", retryLater: "Try {{provider}} again later." },
  setup: {
    eyebrow: "DASHY / SETUP", title: "Choose what Dashy watches", description: "Connect only the tools you use. You can change this later in Settings.",
    languageTitle: "Choose your language", languageDescription: "Dashy switches immediately. You can change this later in Settings.", continue: "Continue", back: "Back", stepLabel: "Step {{current}} of {{total}}",
    useProvider: "Use {{provider}} in Dashy", connected: "Connected", notInstalled: "Not installed", signInRequired: "Sign in required", needsAttention: "Needs attention",
    installing: "Installing", connecting: "Connecting",
    install: "Install {{provider}}", connect: "Connect {{provider}}", retry: "Retry", cancel: "Cancel", confirmInstall: "Confirm installation", confirmLogin: "Open official login",
    installDisclosure: "Dashy will open a visible terminal and run this WinGet command.", installManualDisclosure: "Dashy will open the official install guide in your browser.", loginDisclosure: "Dashy will open the provider's official login in a visible terminal and browser.",
    publisher: "Publisher", packageId: "Package", command: "Command", manualHelp: "Open official installation guide", manualHelpFailure: "Dashy could not open the official installation guide.", finish: "Finish setup",
    finishFailure: "Dashy could not save your provider selection.", actionFailure: "Provider setup needs attention.", loading: "Checking installed tools",
  },
  settings: { title: "Settings", placement: "Placement", right: "Right", left: "Left", top: "Top", monitor: "Monitor", language: "Language", fullscreen: "Always show over fullscreen apps", startup: "Launch at startup", providerStatus: "Provider status" },
  menu: { show: "Show Dashy", refreshAll: "Refresh all providers", placement: "Placement", monitor: "Monitor", primaryMonitor: "Primary monitor", settings: "Settings", quit: "Quit Dashy" },
  actions: { refresh: "Refresh", refreshAll: "Refresh all", openSettings: "Open settings", close: "Close" },
} satisfies Messages;

export default en;
