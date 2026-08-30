import type { Messages } from "./en";

const zhCN = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "当前会话", weeklyWindow: "每周", remaining: "剩余 {{value}}%", resets: "重置时间 {{time}}" },
  github: { streakDays: "连续 {{count}} 天", today: "今天", contributions: "{{count}} 次贡献", heatmapLabel: "过去 12 周的 GitHub 贡献" },
  status: { loading: "正在加载", notInstalled: "未安装", signInRequired: "需要登录", unavailable: "不可用", stale: "上次保存的数据", lastUpdated: "上次更新 {{time}}" },
  guidance: { installClaude: "请安装 Claude CLI，然后重新打开 Dashy。", installCodex: "请安装 Codex CLI，然后重新打开 Dashy。", installGitHub: "请安装 GitHub CLI，然后重新打开 Dashy。", signInClaude: "请登录 Claude 后重试。", signInCodex: "请登录 Codex 后重试。", signInGitHub: "请登录 GitHub 后重试。", retryLater: "请稍后重试 {{provider}}。" },
  settings: { title: "设置", placement: "位置", right: "右侧", left: "左侧", top: "顶部", monitor: "显示器", language: "语言", fullscreen: "始终显示在全屏应用之上", startup: "开机启动", providerStatus: "服务状态" },
  menu: { show: "显示 Dashy", refreshAll: "刷新所有服务", placement: "位置", monitor: "显示器", primaryMonitor: "主显示器", settings: "设置", quit: "退出 Dashy" },
  actions: { refresh: "刷新", refreshAll: "全部刷新", openSettings: "打开设置", close: "关闭" },
} satisfies Messages;

export default zhCN;
