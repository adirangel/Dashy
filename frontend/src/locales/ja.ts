import type { Messages } from "./en";

const ja = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "現在のセッション", weeklyWindow: "週間", remaining: "残り {{value}}%", resets: "リセット {{time}}" },
  github: { streakDays: "{{count}} 日連続", today: "今日", contributions: "{{count}} 件のコントリビューション", heatmapLabel: "過去 12 週間の GitHub コントリビューション" },
  status: { loading: "読み込み中", notInstalled: "未インストール", signInRequired: "サインインが必要です", unavailable: "利用できません", stale: "最後に保存されたデータ", lastUpdated: "最終更新 {{time}}" },
  guidance: { installClaude: "Claude CLI をインストールして Dashy を再度開いてください。", installCodex: "Codex CLI をインストールして Dashy を再度開いてください。", installGitHub: "GitHub CLI をインストールして Dashy を再度開いてください。", signInClaude: "Claude にサインインして再試行してください。", signInCodex: "Codex にサインインして再試行してください。", signInGitHub: "GitHub にサインインして再試行してください。", retryLater: "後でもう一度 {{provider}} をお試しください。" },
  settings: { title: "設定", placement: "配置", right: "右", left: "左", top: "上", monitor: "モニター", language: "言語", fullscreen: "全画面アプリの上にも常に表示", startup: "起動時に開く", providerStatus: "プロバイダーの状態" },
  menu: { show: "Dashy を表示", refreshAll: "すべてのプロバイダーを更新", placement: "配置", monitor: "モニター", primaryMonitor: "メインモニター", settings: "設定", quit: "Dashy を終了" },
  actions: { refresh: "更新", refreshAll: "すべて更新", openSettings: "設定を開く", close: "閉じる" },
} satisfies Messages;

export default ja;
