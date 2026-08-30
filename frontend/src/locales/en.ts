export type Messages = {
  providers: { claude: string; codex: string; github: string };
  usage: { shortWindow: string; weeklyWindow: string; remaining: string; resets: string };
  github: { streakDays: string; today: string; contributions: string; heatmapLabel: string };
  status: { loading: string; notInstalled: string; signInRequired: string; unavailable: string; stale: string; lastUpdated: string };
  guidance: { installClaude: string; installCodex: string; installGitHub: string; signInClaude: string; signInCodex: string; signInGitHub: string; retryLater: string };
  settings: { title: string; placement: string; right: string; left: string; top: string; monitor: string; language: string; fullscreen: string; startup: string; providerStatus: string };
  menu: { show: string; refreshAll: string; placement: string; monitor: string; primaryMonitor: string; settings: string; quit: string };
  actions: { refresh: string; refreshAll: string; openSettings: string; close: string };
};

const en = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "Current session", weeklyWindow: "Weekly", remaining: "{{value}}% remaining", resets: "Resets {{time}}" },
  github: { streakDays: "{{count}} day streak", today: "Today", contributions: "{{count}} contributions", heatmapLabel: "GitHub contributions over the last 12 weeks" },
  status: { loading: "Loading", notInstalled: "Not installed", signInRequired: "Sign in required", unavailable: "Unavailable", stale: "Last known data", lastUpdated: "Last updated {{time}}" },
  guidance: { installClaude: "Install the Claude CLI, then reopen Dashy.", installCodex: "Install the Codex CLI, then reopen Dashy.", installGitHub: "Install the GitHub CLI, then reopen Dashy.", signInClaude: "Sign in to Claude, then retry.", signInCodex: "Sign in to Codex, then retry.", signInGitHub: "Sign in to GitHub, then retry.", retryLater: "Try {{provider}} again later." },
  settings: { title: "Settings", placement: "Placement", right: "Right", left: "Left", top: "Top", monitor: "Monitor", language: "Language", fullscreen: "Always show over fullscreen apps", startup: "Launch at startup", providerStatus: "Provider status" },
  menu: { show: "Show Dashy", refreshAll: "Refresh all providers", placement: "Placement", monitor: "Monitor", primaryMonitor: "Primary monitor", settings: "Settings", quit: "Quit Dashy" },
  actions: { refresh: "Refresh", refreshAll: "Refresh all", openSettings: "Open settings", close: "Close" },
} satisfies Messages;

export default en;
