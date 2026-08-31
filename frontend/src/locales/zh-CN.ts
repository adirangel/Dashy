import type { Messages } from "./en";

const zhCN = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "当前会话", weeklyWindow: "每周", remaining: "剩余 {{value}}%", resets: "重置时间 {{time}}" },
  github: { streakDays: "连续 {{count}} 天", today: "今天", contributions: "{{count}} 次贡献", heatmapLabel: "过去 12 周的 GitHub 贡献" },
  status: { loading: "正在加载", notInstalled: "未安装", signInRequired: "需要登录", unavailable: "不可用", stale: "上次保存的数据", lastUpdated: "上次更新 {{time}}" },
  guidance: { installClaude: "请安装 Claude CLI，然后重新打开 Dashy。", installCodex: "请安装 Codex CLI，然后重新打开 Dashy。", installGitHub: "请安装 GitHub CLI，然后重新打开 Dashy。", signInClaude: "请登录 Claude 后重试。", signInCodex: "请登录 Codex 后重试。", signInGitHub: "请登录 GitHub 后重试。", retryLater: "请稍后重试 {{provider}}。" },
  setup: {
    eyebrow: "DASHY / 设置", title: "选择 Dashy 要监控的内容", description: "只连接你使用的工具。之后可随时在设置中更改。",
    languageTitle: "选择你的语言", languageDescription: "Dashy 会立即切换。之后可随时在设置中更改。", continue: "继续", back: "返回", stepLabel: "第 {{current}} 步，共 {{total}} 步",
    useProvider: "在 Dashy 中使用 {{provider}}", connected: "已连接", notInstalled: "未安装", signInRequired: "需要登录", needsAttention: "需要处理",
    installing: "正在安装", connecting: "正在连接",
    install: "安装 {{provider}}", connect: "连接 {{provider}}", retry: "重试", cancel: "取消", confirmInstall: "确认安装", confirmLogin: "打开官方登录",
    installDisclosure: "Dashy 将打开可见终端并运行此 WinGet 命令。", loginDisclosure: "Dashy 将在可见终端和浏览器中打开服务商的官方登录流程。",
    publisher: "发布者", packageId: "软件包", command: "命令", manualHelp: "打开官方安装指南", manualHelpFailure: "Dashy 无法打开官方安装指南。", finish: "完成设置",
    finishFailure: "Dashy 无法保存服务商选择。", actionFailure: "服务商设置需要处理。", loading: "正在检查已安装的工具",
  },
  settings: { title: "设置", placement: "位置", right: "右侧", left: "左侧", top: "顶部", monitor: "显示器", language: "语言", fullscreen: "始终显示在全屏应用之上", startup: "开机启动", providerStatus: "服务状态" },
  menu: { show: "显示 Dashy", refreshAll: "刷新所有服务", placement: "位置", monitor: "显示器", primaryMonitor: "主显示器", settings: "设置", quit: "退出 Dashy" },
  actions: { refresh: "刷新", refreshAll: "全部刷新", openSettings: "打开设置", close: "关闭" },
} satisfies Messages;

export default zhCN;
