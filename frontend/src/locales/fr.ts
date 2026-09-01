import type { Messages } from "./en";

const fr = {
  providers: { claude: "Claude", codex: "Codex", github: "GitHub", grok: "Grok", cursor: "Cursor" },
  usage: { shortWindow: "Session actuelle", weeklyWindow: "Hebdomadaire", monthlyWindow: "Mensuel", remaining: "{{value}} % restants", resets: "Réinitialisation {{time}}" },
  github: { streakDays: "Série de {{count}} jours", today: "Aujourd’hui", contributions: "{{count}} contributions", heatmapLabel: "Contributions GitHub des 12 dernières semaines" },
  cursor: { plan: "Forfait", account: "Compte", usageHint: "Cursor ne communique pas ses limites d'utilisation. Consultez l'utilisation dans le tableau de bord Cursor." },
  status: { loading: "Chargement", notInstalled: "Non installé", signInRequired: "Connexion requise", unavailable: "Indisponible", stale: "Dernières données enregistrées", lastUpdated: "Dernière mise à jour : {{time}}" },
  guidance: { installClaude: "Installez la CLI Claude, puis rouvrez Dashy.", installCodex: "Installez la CLI Codex, puis rouvrez Dashy.", installGitHub: "Installez la CLI GitHub, puis rouvrez Dashy.", installGrok: "Installez la CLI Grok, puis rouvrez Dashy.", installCursor: "Installez la CLI Cursor, puis rouvrez Dashy.", signInClaude: "Connectez-vous à Claude, puis réessayez.", signInCodex: "Connectez-vous à Codex, puis réessayez.", signInGitHub: "Connectez-vous à GitHub, puis réessayez.", signInGrok: "Connectez-vous à Grok, puis réessayez.", signInCursor: "Connectez-vous à Cursor, puis réessayez.", retryLater: "Réessayez {{provider}} plus tard." },
  setup: {
    eyebrow: "DASHY / CONFIGURATION", title: "Choisissez ce que Dashy surveille", description: "Connectez uniquement les outils que vous utilisez. Vous pourrez modifier ce choix plus tard dans les paramètres.",
    languageTitle: "Choisissez votre langue", languageDescription: "Dashy change immédiatement. Vous pourrez le modifier plus tard dans les paramètres.", continue: "Continuer", back: "Retour", stepLabel: "Étape {{current}} sur {{total}}",
    useProvider: "Utiliser {{provider}} dans Dashy", connected: "Connecté", notInstalled: "Non installé", signInRequired: "Connexion requise", needsAttention: "Nécessite votre attention",
    installing: "Installation", connecting: "Connexion",
    install: "Installer {{provider}}", connect: "Connecter {{provider}}", retry: "Réessayer", cancel: "Annuler", confirmInstall: "Confirmer l’installation", confirmLogin: "Ouvrir la connexion officielle",
    installDisclosure: "Dashy ouvrira un terminal visible et exécutera cette commande WinGet.", installManualDisclosure: "Dashy ouvrira le guide d'installation officiel dans votre navigateur.", loginDisclosure: "Dashy ouvrira la connexion officielle du fournisseur dans un terminal visible et le navigateur.",
    publisher: "Éditeur", packageId: "Paquet", command: "Commande", manualHelp: "Ouvrir le guide d’installation officiel", manualHelpFailure: "Dashy n’a pas pu ouvrir le guide d’installation officiel.", finish: "Terminer la configuration",
    finishFailure: "Dashy n’a pas pu enregistrer la sélection des fournisseurs.", actionFailure: "La configuration du fournisseur nécessite votre attention.", loading: "Vérification des outils installés",
  },
  settings: { title: "Paramètres", placement: "Position", right: "Droite", left: "Gauche", top: "Haut", monitor: "Écran", language: "Langue", fullscreen: "Toujours afficher au-dessus des applications en plein écran", startup: "Lancer au démarrage", providerStatus: "État des services" },
  menu: { show: "Afficher Dashy", refreshAll: "Actualiser tous les services", placement: "Position", monitor: "Écran", primaryMonitor: "Écran principal", settings: "Paramètres", quit: "Quitter Dashy" },
  actions: { refresh: "Actualiser", refreshAll: "Tout actualiser", openSettings: "Ouvrir les paramètres", close: "Fermer" },
} satisfies Messages;

export default fr;
