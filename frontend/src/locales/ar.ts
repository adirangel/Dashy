import type { Messages } from "./en";

const ar = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub", grok: "Grok", cursor: "Cursor" },
  usage: { shortWindow: "الجلسة الحالية", weeklyWindow: "أسبوعي", monthlyWindow: "شهري", remaining: "متبقٍ {{value}}٪", resets: "تُعاد التهيئة {{time}}" },
  github: { streakDays: "سلسلة {{count}} أيام", today: "اليوم", contributions: "{{count}} مساهمات", heatmapLabel: "مساهمات GitHub خلال آخر 12 أسبوعًا", streakUnit: "يوم متتالي", todayUnit: "مساهمات اليوم" },
  cursor: { plan: "الخطة", account: "الحساب", usageHint: "لا يوفر Cursor حدود الاستخدام. راجع الاستخدام في لوحة تحكم Cursor." },
  status: { loading: "جارٍ التحميل", notInstalled: "غير مثبّت", signInRequired: "يلزم تسجيل الدخول", unavailable: "غير متاح", stale: "آخر بيانات محفوظة", lastUpdated: "آخر تحديث {{time}}" },
  guidance: { installClaude: "ثبّت أداة Claude لسطر الأوامر ثم أعد فتح Dashy.", installCodex: "ثبّت أداة Codex لسطر الأوامر ثم أعد فتح Dashy.", installGitHub: "ثبّت أداة GitHub لسطر الأوامر ثم أعد فتح Dashy.", installGrok: "ثبّت أداة Grok لسطر الأوامر ثم أعد فتح Dashy.", installCursor: "ثبّت أداة Cursor لسطر الأوامر ثم أعد فتح Dashy.", signInClaude: "سجّل الدخول إلى Claude ثم أعد المحاولة.", signInCodex: "سجّل الدخول إلى Codex ثم أعد المحاولة.", signInGitHub: "سجّل الدخول إلى GitHub ثم أعد المحاولة.", signInGrok: "سجّل الدخول إلى Grok ثم أعد المحاولة.", signInCursor: "سجّل الدخول إلى Cursor ثم أعد المحاولة.", retryLater: "جرّب {{provider}} مرة أخرى لاحقًا.", usageUnavailable: "تم تسجيل الدخول إلى {{provider}} لكن لم تُرجع الأداة حدود الاستخدام. راجع الاستخدام في حسابك لدى المزوّد.", unsupportedOutput: "تعذر على Dashy قراءة استجابة أداة {{provider}} لسطر الأوامر. تحقق من تحديثات Dashy والأداة ثم أعد المحاولة.", timeout: "استغرقت أداة {{provider}} لسطر الأوامر وقتًا طويلًا للاستجابة. تحقق من اتصالك ثم أعد المحاولة.", launch: "تعذر على Dashy تشغيل أداة {{provider}} لسطر الأوامر. تحقق من تشغيلها في الطرفية ثم أعد فتح Dashy.", process: "فشل الطلب عبر أداة {{provider}} لسطر الأوامر. شغّلها في الطرفية لفحص المشكلة ثم أعد المحاولة.", network: "تعذر على أداة {{provider}} لسطر الأوامر الوصول إلى الخدمة. تحقق من اتصالك ثم أعد المحاولة." },
  setup: {
    eyebrow: "DASHY / الإعداد", title: "اختر ما يراقبه Dashy", description: "صِل الأدوات التي تستخدمها فقط. يمكنك تغيير ذلك لاحقًا من الإعدادات.",
    languageTitle: "اختر لغتك", languageDescription: "سيتبدّل Dashy فورًا. يمكنك تغيير ذلك لاحقًا من الإعدادات.", continue: "متابعة", back: "رجوع", stepLabel: "الخطوة {{current}} من {{total}}",
    useProvider: "استخدام {{provider}} في Dashy", connected: "متصل", notInstalled: "غير مثبت", signInRequired: "يلزم تسجيل الدخول", needsAttention: "يحتاج إلى إجراء",
    installing: "جارٍ التثبيت", connecting: "جارٍ الاتصال",
    install: "تثبيت {{provider}}", connect: "ربط {{provider}}", retry: "إعادة المحاولة", cancel: "إلغاء", confirmInstall: "تأكيد التثبيت", confirmLogin: "فتح تسجيل الدخول الرسمي",
    installDisclosure: "سيفتح Dashy طرفية ظاهرة وينفذ هذا الأمر.", installManualDisclosure: "سيفتح Dashy دليل التثبيت الرسمي في المتصفح.", loginDisclosure: "سيفتح Dashy تسجيل الدخول الرسمي للموفر في طرفية ظاهرة والمتصفح.",
    publisher: "الناشر", packageId: "الحزمة", command: "الأمر", manualHelp: "فتح دليل التثبيت الرسمي", manualHelpFailure: "تعذر على Dashy فتح دليل التثبيت الرسمي.", finish: "إنهاء الإعداد",
    usageUnavailable: "متصل · بيانات الاستخدام غير متاحة", providersNotReady: "غير جاهز بعد: {{providers}}. ثبّت الأدوات أو سجّل الدخول أو أعد المحاولة، أو ألغِ تحديدها للإنهاء.", defer: "حفظ والإكمال لاحقًا", deferHint: "سيبقى الإعداد غير مكتمل وDashy مخفيًا. للمتابعة اختر إظهار Dashy من علبة النظام.", finishNotReady: "تعذر التحقق من أحد المزوّدين المحددين. راجع حالته وأعد المحاولة.",
    finishFailure: "تعذر على Dashy حفظ اختيار الموفرين.", actionFailure: "يحتاج إعداد الموفر إلى إجراء.", loading: "جارٍ فحص الأدوات المثبتة",
  },
  settings: { title: "الإعدادات", placement: "الموضع", right: "يمين", left: "يسار", top: "أعلى", monitor: "الشاشة", language: "اللغة", fullscreen: "العرض دائمًا فوق تطبيقات ملء الشاشة", startup: "التشغيل عند بدء النظام", display: "العرض", providers: "المزوّدون", diagnostics: "التشخيص", diagnosticsHint: "سجل محلي لتحديثات المزوّدين: أي أداة عملت، كم استغرقت، وهل نجحت. بدون مخرجات أو أسرار.", openLogFolder: "فتح مجلد السجل" },
  menu: { show: "إظهار Dashy", refreshAll: "تحديث جميع المزوّدين", placement: "الموضع", monitor: "الشاشة", primaryMonitor: "الشاشة الرئيسية", settings: "الإعدادات", quit: "إنهاء Dashy" },
  actions: { refreshAll: "تحديث الكل" },
} satisfies Messages;

export default ar;
