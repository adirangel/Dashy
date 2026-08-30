import type { Messages } from "./en";

const he = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "הפעלה נוכחית", weeklyWindow: "שבועי", remaining: "נותרו {{value}}%", resets: "מתאפס {{time}}" },
  github: { streakDays: "רצף של {{count}} ימים", today: "היום", contributions: "{{count}} תרומות", heatmapLabel: "תרומות GitHub ב־12 השבועות האחרונים" },
  status: { loading: "טוען", notInstalled: "לא מותקן", signInRequired: "נדרשת התחברות", unavailable: "לא זמין", stale: "נתון אחרון שנשמר", lastUpdated: "עודכן לאחרונה {{time}}" },
  guidance: { installClaude: "התקינו את כלי שורת הפקודה של Claude ופתחו שוב את Dashy.", installCodex: "התקינו את כלי שורת הפקודה של Codex ופתחו שוב את Dashy.", installGitHub: "התקינו את כלי שורת הפקודה של GitHub ופתחו שוב את Dashy.", signInClaude: "התחברו ל־Claude ונסו שוב.", signInCodex: "התחברו ל־Codex ונסו שוב.", signInGitHub: "התחברו ל־GitHub ונסו שוב.", retryLater: "נסו את {{provider}} שוב מאוחר יותר." },
  settings: { title: "הגדרות", placement: "מיקום", right: "ימין", left: "שמאל", top: "למעלה", monitor: "צג", language: "שפה", fullscreen: "הצג תמיד מעל יישומים במסך מלא", startup: "הפעל בעת האתחול", providerStatus: "מצב ספקים" },
  menu: { show: "הצג את Dashy", refreshAll: "רענן את כל הספקים", placement: "מיקום", monitor: "צג", primaryMonitor: "צג ראשי", settings: "הגדרות", quit: "צא מ־Dashy" },
  actions: { refresh: "רענן", refreshAll: "רענן הכול", openSettings: "פתח הגדרות", close: "סגור" },
} satisfies Messages;

export default he;
