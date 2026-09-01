import type { Messages } from "./en";

const ru = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub", grok: "Grok", cursor: "Cursor" },
  usage: { shortWindow: "Текущая сессия", weeklyWindow: "За неделю", monthlyWindow: "За месяц", remaining: "Осталось {{value}} %", resets: "Сброс: {{time}}" },
  github: { streakDays: "Серия: {{count}} дн.", today: "Сегодня", contributions: "Вкладов: {{count}}", heatmapLabel: "Вклады GitHub за последние 12 недель", streakUnit: "дней подряд", todayUnit: "вкладов сегодня" },
  cursor: { plan: "Тариф", account: "Аккаунт", usageHint: "Cursor не сообщает лимиты использования. Смотрите статистику в панели Cursor." },
  status: { loading: "Загрузка", notInstalled: "Не установлено", signInRequired: "Требуется вход", unavailable: "Недоступно", stale: "Последние сохранённые данные", lastUpdated: "Обновлено {{time}}" },
  guidance: { installClaude: "Установите Claude CLI и снова откройте Dashy.", installCodex: "Установите Codex CLI и снова откройте Dashy.", installGitHub: "Установите GitHub CLI и снова откройте Dashy.", installGrok: "Установите Grok CLI и снова откройте Dashy.", installCursor: "Установите Cursor CLI и снова откройте Dashy.", signInClaude: "Войдите в Claude и повторите попытку.", signInCodex: "Войдите в Codex и повторите попытку.", signInGitHub: "Войдите в GitHub и повторите попытку.", signInGrok: "Войдите в Grok и повторите попытку.", signInCursor: "Войдите в Cursor и повторите попытку.", retryLater: "Повторите попытку с {{provider}} позже." },
  setup: {
    eyebrow: "DASHY / НАСТРОЙКА", title: "Выберите, что отслеживает Dashy", description: "Подключите только те инструменты, которыми пользуетесь. Это можно изменить позже в настройках.",
    languageTitle: "Выберите язык", languageDescription: "Dashy переключится сразу. Это можно изменить позже в настройках.", continue: "Продолжить", back: "Назад", stepLabel: "Шаг {{current}} из {{total}}",
    useProvider: "Использовать {{provider}} в Dashy", connected: "Подключено", notInstalled: "Не установлено", signInRequired: "Требуется вход", needsAttention: "Требует внимания",
    installing: "Установка", connecting: "Подключение",
    install: "Установить {{provider}}", connect: "Подключить {{provider}}", retry: "Повторить", cancel: "Отмена", confirmInstall: "Подтвердить установку", confirmLogin: "Открыть официальный вход",
    installDisclosure: "Dashy откроет видимый терминал и выполнит эту команду WinGet.", installManualDisclosure: "Dashy откроет официальное руководство по установке в браузере.", loginDisclosure: "Dashy откроет официальный вход провайдера в видимом терминале и браузере.",
    publisher: "Издатель", packageId: "Пакет", command: "Команда", manualHelp: "Открыть официальное руководство по установке", manualHelpFailure: "Dashy не удалось открыть официальное руководство по установке.", finish: "Завершить настройку",
    finishFailure: "Dashy не удалось сохранить выбор провайдеров.", actionFailure: "Настройка провайдера требует внимания.", loading: "Проверка установленных инструментов",
  },
  settings: { title: "Настройки", placement: "Положение", right: "Справа", left: "Слева", top: "Сверху", monitor: "Монитор", language: "Язык", fullscreen: "Всегда показывать поверх полноэкранных приложений", startup: "Запускать при входе", display: "Экран", providers: "Сервисы", diagnostics: "Диагностика", diagnosticsHint: "Локальный журнал обновлений сервисов: какой CLI запускался, сколько занял и удался ли. Без вывода и секретов.", openLogFolder: "Открыть папку журнала" },
  menu: { show: "Показать Dashy", refreshAll: "Обновить все сервисы", placement: "Положение", monitor: "Монитор", primaryMonitor: "Основной монитор", settings: "Настройки", quit: "Выйти из Dashy" },
  actions: { refreshAll: "Обновить всё" },
} satisfies Messages;

export default ru;
