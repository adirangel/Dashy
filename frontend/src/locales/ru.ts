import type { Messages } from "./en";

const ru = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "Текущая сессия", weeklyWindow: "За неделю", remaining: "Осталось {{value}} %", resets: "Сброс: {{time}}" },
  github: { streakDays: "Серия: {{count}} дн.", today: "Сегодня", contributions: "Вкладов: {{count}}", heatmapLabel: "Вклады GitHub за последние 12 недель" },
  status: { loading: "Загрузка", notInstalled: "Не установлено", signInRequired: "Требуется вход", unavailable: "Недоступно", stale: "Последние сохранённые данные", lastUpdated: "Обновлено {{time}}" },
  guidance: { installClaude: "Установите Claude CLI и снова откройте Dashy.", installCodex: "Установите Codex CLI и снова откройте Dashy.", installGitHub: "Установите GitHub CLI и снова откройте Dashy.", signInClaude: "Войдите в Claude и повторите попытку.", signInCodex: "Войдите в Codex и повторите попытку.", signInGitHub: "Войдите в GitHub и повторите попытку.", retryLater: "Повторите попытку с {{provider}} позже." },
  settings: { title: "Настройки", placement: "Положение", right: "Справа", left: "Слева", top: "Сверху", monitor: "Монитор", language: "Язык", fullscreen: "Всегда показывать поверх полноэкранных приложений", startup: "Запускать при входе", providerStatus: "Состояние сервисов" },
  menu: { show: "Показать Dashy", refreshAll: "Обновить все сервисы", placement: "Положение", monitor: "Монитор", primaryMonitor: "Основной монитор", settings: "Настройки", quit: "Выйти из Dashy" },
  actions: { refresh: "Обновить", refreshAll: "Обновить всё", openSettings: "Открыть настройки", close: "Закрыть" },
} satisfies Messages;

export default ru;
