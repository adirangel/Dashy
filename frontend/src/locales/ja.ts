import type { Messages } from "./en";

const ja = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub", grok: "Grok", cursor: "Cursor" },
  usage: { shortWindow: "現在のセッション", weeklyWindow: "週間", monthlyWindow: "月間", remaining: "残り {{value}}%", resets: "リセット {{time}}" },
  github: { streakDays: "{{count}} 日連続", today: "今日", contributions: "{{count}} 件のコントリビューション", heatmapLabel: "過去 12 週間の GitHub コントリビューション" },
  cursor: { plan: "プラン", account: "アカウント", usageHint: "Cursor は使用量の上限を公開していません。使用状況は Cursor のダッシュボードで確認してください。" },
  status: { loading: "読み込み中", notInstalled: "未インストール", signInRequired: "サインインが必要です", unavailable: "利用できません", stale: "最後に保存されたデータ", lastUpdated: "最終更新 {{time}}" },
  guidance: { installClaude: "Claude CLI をインストールして Dashy を再度開いてください。", installCodex: "Codex CLI をインストールして Dashy を再度開いてください。", installGitHub: "GitHub CLI をインストールして Dashy を再度開いてください。", installGrok: "Grok CLI をインストールして Dashy を再度開いてください。", installCursor: "Cursor CLI をインストールして Dashy を再度開いてください。", signInClaude: "Claude にサインインして再試行してください。", signInCodex: "Codex にサインインして再試行してください。", signInGitHub: "GitHub にサインインして再試行してください。", signInGrok: "Grok にサインインして再試行してください。", signInCursor: "Cursor にサインインして再試行してください。", retryLater: "後でもう一度 {{provider}} をお試しください。" },
  setup: {
    eyebrow: "DASHY / セットアップ", title: "Dashy で確認するサービスを選択", description: "使用するツールだけを接続してください。後で設定から変更できます。",
    languageTitle: "言語を選択してください", languageDescription: "Dashy はすぐに切り替わります。後で設定から変更できます。", continue: "続行", back: "戻る", stepLabel: "ステップ {{current}} / {{total}}",
    useProvider: "Dashy で {{provider}} を使用", connected: "接続済み", notInstalled: "未インストール", signInRequired: "サインインが必要", needsAttention: "確認が必要",
    installing: "インストール中", connecting: "接続中",
    install: "{{provider}} をインストール", connect: "{{provider}} に接続", retry: "再試行", cancel: "キャンセル", confirmInstall: "インストールを確認", confirmLogin: "公式ログインを開く",
    installDisclosure: "Dashy は表示されたターミナルを開き、この WinGet コマンドを実行します。", installManualDisclosure: "Dashy はブラウザーで公式インストールガイドを開きます。", loginDisclosure: "Dashy は表示されたターミナルとブラウザーでプロバイダーの公式ログインを開きます。",
    publisher: "発行元", packageId: "パッケージ", command: "コマンド", manualHelp: "公式インストールガイドを開く", manualHelpFailure: "Dashy は公式インストールガイドを開けませんでした。", finish: "セットアップを完了",
    finishFailure: "Dashy はプロバイダーの選択を保存できませんでした。", actionFailure: "プロバイダーのセットアップを確認してください。", loading: "インストール済みツールを確認中",
  },
  settings: { title: "設定", placement: "配置", right: "右", left: "左", top: "上", monitor: "モニター", language: "言語", fullscreen: "全画面アプリの上にも常に表示", startup: "起動時に開く", providerStatus: "プロバイダーの状態" },
  menu: { show: "Dashy を表示", refreshAll: "すべてのプロバイダーを更新", placement: "配置", monitor: "モニター", primaryMonitor: "メインモニター", settings: "設定", quit: "Dashy を終了" },
  actions: { refresh: "更新", refreshAll: "すべて更新", openSettings: "設定を開く", close: "閉じる" },
} satisfies Messages;

export default ja;
