import type { Messages } from "./en";

const ar = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "الجلسة الحالية", weeklyWindow: "أسبوعي", remaining: "متبقٍ {{value}}٪", resets: "تُعاد التهيئة {{time}}" },
  github: { streakDays: "سلسلة {{count}} أيام", today: "اليوم", contributions: "{{count}} مساهمات", heatmapLabel: "مساهمات GitHub خلال آخر 12 أسبوعًا" },
  status: { loading: "جارٍ التحميل", notInstalled: "غير مثبّت", signInRequired: "يلزم تسجيل الدخول", unavailable: "غير متاح", stale: "آخر بيانات محفوظة", lastUpdated: "آخر تحديث {{time}}" },
  guidance: { installClaude: "ثبّت أداة Claude لسطر الأوامر ثم أعد فتح Dashy.", installCodex: "ثبّت أداة Codex لسطر الأوامر ثم أعد فتح Dashy.", installGitHub: "ثبّت أداة GitHub لسطر الأوامر ثم أعد فتح Dashy.", signInClaude: "سجّل الدخول إلى Claude ثم أعد المحاولة.", signInCodex: "سجّل الدخول إلى Codex ثم أعد المحاولة.", signInGitHub: "سجّل الدخول إلى GitHub ثم أعد المحاولة.", retryLater: "جرّب {{provider}} مرة أخرى لاحقًا." },
  settings: { title: "الإعدادات", placement: "الموضع", right: "يمين", left: "يسار", top: "أعلى", monitor: "الشاشة", language: "اللغة", fullscreen: "العرض دائمًا فوق تطبيقات ملء الشاشة", startup: "التشغيل عند بدء النظام", providerStatus: "حالة المزوّدين" },
  menu: { show: "إظهار Dashy", refreshAll: "تحديث جميع المزوّدين", placement: "الموضع", monitor: "الشاشة", primaryMonitor: "الشاشة الرئيسية", settings: "الإعدادات", quit: "إنهاء Dashy" },
  actions: { refresh: "تحديث", refreshAll: "تحديث الكل", openSettings: "فتح الإعدادات", close: "إغلاق" },
} satisfies Messages;

export default ar;
