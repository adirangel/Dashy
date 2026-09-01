import type { Messages } from "./en";

const ar = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub", grok: "Grok", cursor: "Cursor" },
  usage: { shortWindow: "الجلسة الحالية", weeklyWindow: "أسبوعي", monthlyWindow: "شهري", remaining: "متبقٍ {{value}}٪", resets: "تُعاد التهيئة {{time}}" },
  github: { streakDays: "سلسلة {{count}} أيام", today: "اليوم", contributions: "{{count}} مساهمات", heatmapLabel: "مساهمات GitHub خلال آخر 12 أسبوعًا" },
  cursor: { plan: "الخطة", account: "الحساب", usageHint: "لا يوفر Cursor حدود الاستخدام. راجع الاستخدام في لوحة تحكم Cursor." },
  status: { loading: "جارٍ التحميل", notInstalled: "غير مثبّت", signInRequired: "يلزم تسجيل الدخول", unavailable: "غير متاح", stale: "آخر بيانات محفوظة", lastUpdated: "آخر تحديث {{time}}" },
  guidance: { installClaude: "ثبّت أداة Claude لسطر الأوامر ثم أعد فتح Dashy.", installCodex: "ثبّت أداة Codex لسطر الأوامر ثم أعد فتح Dashy.", installGitHub: "ثبّت أداة GitHub لسطر الأوامر ثم أعد فتح Dashy.", installGrok: "ثبّت أداة Grok لسطر الأوامر ثم أعد فتح Dashy.", installCursor: "ثبّت أداة Cursor لسطر الأوامر ثم أعد فتح Dashy.", signInClaude: "سجّل الدخول إلى Claude ثم أعد المحاولة.", signInCodex: "سجّل الدخول إلى Codex ثم أعد المحاولة.", signInGitHub: "سجّل الدخول إلى GitHub ثم أعد المحاولة.", signInGrok: "سجّل الدخول إلى Grok ثم أعد المحاولة.", signInCursor: "سجّل الدخول إلى Cursor ثم أعد المحاولة.", retryLater: "جرّب {{provider}} مرة أخرى لاحقًا." },
  setup: {
    eyebrow: "DASHY / الإعداد", title: "اختر ما يراقبه Dashy", description: "صِل الأدوات التي تستخدمها فقط. يمكنك تغيير ذلك لاحقًا من الإعدادات.",
    languageTitle: "اختر لغتك", languageDescription: "سيتبدّل Dashy فورًا. يمكنك تغيير ذلك لاحقًا من الإعدادات.", continue: "متابعة", back: "رجوع", stepLabel: "الخطوة {{current}} من {{total}}",
    useProvider: "استخدام {{provider}} في Dashy", connected: "متصل", notInstalled: "غير مثبت", signInRequired: "يلزم تسجيل الدخول", needsAttention: "يحتاج إلى إجراء",
    installing: "جارٍ التثبيت", connecting: "جارٍ الاتصال",
    install: "تثبيت {{provider}}", connect: "ربط {{provider}}", retry: "إعادة المحاولة", cancel: "إلغاء", confirmInstall: "تأكيد التثبيت", confirmLogin: "فتح تسجيل الدخول الرسمي",
    installDisclosure: "سيفتح Dashy طرفية ظاهرة وينفذ أمر WinGet هذا.", installManualDisclosure: "سيفتح Dashy دليل التثبيت الرسمي في المتصفح.", loginDisclosure: "سيفتح Dashy تسجيل الدخول الرسمي للموفر في طرفية ظاهرة والمتصفح.",
    publisher: "الناشر", packageId: "الحزمة", command: "الأمر", manualHelp: "فتح دليل التثبيت الرسمي", manualHelpFailure: "تعذر على Dashy فتح دليل التثبيت الرسمي.", finish: "إنهاء الإعداد",
    finishFailure: "تعذر على Dashy حفظ اختيار الموفرين.", actionFailure: "يحتاج إعداد الموفر إلى إجراء.", loading: "جارٍ فحص الأدوات المثبتة",
  },
  settings: { title: "الإعدادات", placement: "الموضع", right: "يمين", left: "يسار", top: "أعلى", monitor: "الشاشة", language: "اللغة", fullscreen: "العرض دائمًا فوق تطبيقات ملء الشاشة", startup: "التشغيل عند بدء النظام", display: "العرض", providers: "المزوّدون" },
  menu: { show: "إظهار Dashy", refreshAll: "تحديث جميع المزوّدين", placement: "الموضع", monitor: "الشاشة", primaryMonitor: "الشاشة الرئيسية", settings: "الإعدادات", quit: "إنهاء Dashy" },
  actions: { refreshAll: "تحديث الكل" },
} satisfies Messages;

export default ar;
