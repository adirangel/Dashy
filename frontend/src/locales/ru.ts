import type { Messages } from "./en";

const ru = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "Текущая сессия", weeklyWindow: "За неделю", remaining: "Осталось {{value}} %", resets: "Сброс: {{time}}" },
  github: { streakDays: "Серия: {{count}} дн.", today: "Сегодня", contributions: "Вкладов: {{count}}", heatmapLabel: "Вклады GitHub за последние 12 недель" },
  status: { loading: "Загрузка", notInstalled: "Не установлено", signInRequired: "Требуется вход", unavailable: "Недоступно", stale: "Последние сохранённые данные", lastUpdated: "Обновлено {{time}}" },
  guidance: { installClaude: "Установите Claude CLI и снова откройте Dashy.", installCodex: "Установите Codex CLI и снова откройте Dashy.", installGitHub: "Установите GitHub CLI и снова откройте Dashy.", signInClaude: "Войдите в Claude и повторите попытку.", signInCodex: "Войдите в Codex и повторите попытку.", signInGitHub: "Войдите в GitHub и повторите попытку.", retryLater: "Повторите попытку с {{provider}} позже." },
  setup: {
    eyebrow: "DASHY / НАСТРОЙКА", title: "Выберите, что отслеживает Dashy", description: "Подключите только те инструменты, которыми пользуетесь. Это можно изменить позже в настройках.",
    languageTitle: "Выберите язык", languageDescription: "Dashy переключится сразу. Это можно изменить позже в настройках.", continue: "Продолжить", back: "Назад", stepLabel: "Шаг {{current}} из {{total}}",
    useProvider: "Использовать {{provider}} в Dashy", connected: "Подключено", notInstalled: "Не установлено", signInRequired: "Требуется вход", needsAttention: "Требует внимания",
    installing: "Установка", connecting: "Подключение",
    install: "Установить {{provider}}", connect: "Подключить {{provider}}", retry: "Повторить", cancel: "Отмена", confirmInstall: "Подтвердить установку", confirmLogin: "Открыть официальный вход",
    installDisclosure: "Dashy откроет видимый терминал и выполнит эту команду WinGet.", loginDisclosure: "Dashy откроет официальный вход провайдера в видимом терминале и браузере.",
    publisher: "Издатель", packageId: "Пакет", command: "Команда", manualHelp: "Открыть официальное руководство по установке", manualHelpFailure: "Dashy не удалось открыть официальное руководство по установке.", finish: "Завершить настройку",
    finishFailure: "Dashy не удалось сохранить выбор провайдеров.", actionFailure: "Настройка провайдера требует внимания.", loading: "Проверка установленных инструментов",
  },
  settings: { title: "Настройки", placement: "Положение", right: "Справа", left: "Слева", top: "Сверху", monitor: "Монитор", language: "Язык", fullscreen: "Всегда показывать поверх полноэкранных приложений", startup: "Запускать при входе", providerStatus: "Состояние сервисов" },
  menu: { show: "Показать Dashy", refreshAll: "Обновить все сервисы", placement: "Положение", monitor: "Монитор", primaryMonitor: "Основной монитор", settings: "Настройки", quit: "Выйти из Dashy" },
  actions: { refresh: "Обновить", refreshAll: "Обновить всё", openSettings: "Открыть настройки", close: "Закрыть" },
} satisfies Messages;

export default ru;
