import type { Messages } from "./en";

const es = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub" },
  usage: { shortWindow: "Sesión actual", weeklyWindow: "Semanal", remaining: "{{value}} % restante", resets: "Se restablece {{time}}" },
  github: { streakDays: "Racha de {{count}} días", today: "Hoy", contributions: "{{count}} contribuciones", heatmapLabel: "Contribuciones de GitHub de las últimas 12 semanas" },
  status: { loading: "Cargando", notInstalled: "No instalado", signInRequired: "Debes iniciar sesión", unavailable: "No disponible", stale: "Últimos datos guardados", lastUpdated: "Última actualización: {{time}}" },
  guidance: { installClaude: "Instala la CLI de Claude y vuelve a abrir Dashy.", installCodex: "Instala la CLI de Codex y vuelve a abrir Dashy.", installGitHub: "Instala la CLI de GitHub y vuelve a abrir Dashy.", signInClaude: "Inicia sesión en Claude y vuelve a intentarlo.", signInCodex: "Inicia sesión en Codex y vuelve a intentarlo.", signInGitHub: "Inicia sesión en GitHub y vuelve a intentarlo.", retryLater: "Vuelve a probar {{provider}} más tarde." },
  settings: { title: "Configuración", placement: "Posición", right: "Derecha", left: "Izquierda", top: "Arriba", monitor: "Monitor", language: "Idioma", fullscreen: "Mostrar siempre sobre aplicaciones a pantalla completa", startup: "Abrir al iniciar", providerStatus: "Estado de proveedores" },
  menu: { show: "Mostrar Dashy", refreshAll: "Actualizar todos los proveedores", placement: "Posición", monitor: "Monitor", primaryMonitor: "Monitor principal", settings: "Configuración", quit: "Salir de Dashy" },
  actions: { refresh: "Actualizar", refreshAll: "Actualizar todo", openSettings: "Abrir configuración", close: "Cerrar" },
} satisfies Messages;

export default es;
