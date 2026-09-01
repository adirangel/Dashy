import type { Messages } from "./en";

const he = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub", grok: "Grok", cursor: "Cursor" },
  usage: { shortWindow: "הפעלה נוכחית", weeklyWindow: "שבועי", monthlyWindow: "חודשי", remaining: "נותרו {{value}}%", resets: "מתאפס {{time}}" },
  github: { streakDays: "רצף של {{count}} ימים", today: "היום", contributions: "{{count}} תרומות", heatmapLabel: "תרומות GitHub ב־12 השבועות האחרונים" },
  cursor: { plan: "מסלול", account: "חשבון", usageHint: "Cursor אינו מדווח על מגבלות שימוש. בדקו את השימוש בלוח הבקרה של Cursor." },
  status: { loading: "טוען", notInstalled: "לא מותקן", signInRequired: "נדרשת התחברות", unavailable: "לא זמין", stale: "נתון אחרון שנשמר", lastUpdated: "עודכן לאחרונה {{time}}" },
  guidance: { installClaude: "התקינו את כלי שורת הפקודה של Claude ופתחו שוב את Dashy.", installCodex: "התקינו את כלי שורת הפקודה של Codex ופתחו שוב את Dashy.", installGitHub: "התקינו את כלי שורת הפקודה של GitHub ופתחו שוב את Dashy.", installGrok: "התקינו את כלי שורת הפקודה של Grok ופתחו שוב את Dashy.", installCursor: "התקינו את כלי שורת הפקודה של Cursor ופתחו שוב את Dashy.", signInClaude: "התחברו ל־Claude ונסו שוב.", signInCodex: "התחברו ל־Codex ונסו שוב.", signInGitHub: "התחברו ל־GitHub ונסו שוב.", signInGrok: "התחברו ל־Grok ונסו שוב.", signInCursor: "התחברו ל־Cursor ונסו שוב.", retryLater: "נסו את {{provider}} שוב מאוחר יותר." },
  setup: {
    eyebrow: "DASHY / הגדרה", title: "בחרו במה Dashy יצפה", description: "חברו רק את הכלים שבהם אתם משתמשים. אפשר לשנות זאת מאוחר יותר בהגדרות.",
    languageTitle: "בחרו את השפה שלכם", languageDescription: "Dashy יעבור לשפה מיד. אפשר לשנות זאת מאוחר יותר בהגדרות.", continue: "המשך", back: "חזרה", stepLabel: "שלב {{current}} מתוך {{total}}",
    useProvider: "השתמשו ב־{{provider}} ב־Dashy", connected: "מחובר", notInstalled: "לא מותקן", signInRequired: "נדרשת התחברות", needsAttention: "דורש טיפול",
    installing: "מתבצעת התקנה", connecting: "מתבצע חיבור",
    install: "התקנת {{provider}}", connect: "חיבור {{provider}}", retry: "ניסיון חוזר", cancel: "ביטול", confirmInstall: "אישור ההתקנה", confirmLogin: "פתיחת ההתחברות הרשמית",
    installDisclosure: "Dashy יפתח מסוף גלוי ויריץ את פקודת WinGet הזאת.", installManualDisclosure: "Dashy יפתח את מדריך ההתקנה הרשמי בדפדפן.", loginDisclosure: "Dashy יפתח את ההתחברות הרשמית של הספק במסוף גלוי ובדפדפן.",
    publisher: "מפרסם", packageId: "חבילה", command: "פקודה", manualHelp: "פתיחת מדריך ההתקנה הרשמי", manualHelpFailure: "Dashy לא הצליח לפתוח את מדריך ההתקנה הרשמי.", finish: "סיום ההגדרה",
    finishFailure: "Dashy לא הצליח לשמור את בחירת הספקים.", actionFailure: "הגדרת הספק דורשת טיפול.", loading: "בודק כלים מותקנים",
  },
  settings: { title: "הגדרות", placement: "מיקום", right: "ימין", left: "שמאל", top: "למעלה", monitor: "צג", language: "שפה", fullscreen: "הצג תמיד מעל יישומים במסך מלא", startup: "הפעל בעת האתחול", providerStatus: "מצב ספקים" },
  menu: { show: "הצג את Dashy", refreshAll: "רענן את כל הספקים", placement: "מיקום", monitor: "צג", primaryMonitor: "צג ראשי", settings: "הגדרות", quit: "צא מ־Dashy" },
  actions: { refreshAll: "רענן הכול" },
} satisfies Messages;

export default he;
