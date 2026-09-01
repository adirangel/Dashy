use std::sync::{Arc, RwLock};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use tauri::{Manager, Runtime};
use tauri_plugin_store::StoreExt;
use tokio::sync::watch;

use crate::dashboard::models::ProviderId;

use super::platform::MonitorDescriptor;

const SETTINGS_STORE_FILE: &str = "settings.json";
const SETTINGS_STORE_KEY: &str = "settings";
const MAX_MONITOR_TEXT_LENGTH: usize = 256;
/// Increment this when existing installations must review provider selection again.
pub const CURRENT_PROVIDER_SETUP_VERSION: u16 = 3;

fn legacy_onboarding_completed() -> bool {
    true
}

fn legacy_enabled_providers() -> Vec<ProviderId> {
    // Files predating the enabled-providers key were written when exactly these
    // three providers existed; migration must not silently pre-enable newer ones.
    vec![ProviderId::Claude, ProviderId::Codex, ProviderId::GitHub]
}

fn legacy_provider_setup_version() -> u16 {
    0
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgePlacement {
    Right,
    Left,
    Top,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocaleCode {
    En,
    He,
    Ar,
    Es,
    Ru,
    Fr,
    #[serde(rename = "zh-CN")]
    ZhCn,
    Ja,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorPreference {
    pub id: String,
    pub name: String,
    pub last_work_area: StoredMonitorRect,
}

impl From<&MonitorDescriptor> for MonitorPreference {
    fn from(monitor: &MonitorDescriptor) -> Self {
        Self {
            id: monitor.id.clone(),
            name: monitor.name.clone(),
            last_work_area: StoredMonitorRect {
                x: monitor.work_rect.x(),
                y: monitor.work_rect.y(),
                width: monitor.work_rect.width(),
                height: monitor.work_rect.height(),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub placement: EdgePlacement,
    pub monitor: Option<MonitorPreference>,
    pub locale: LocaleCode,
    pub always_show_over_fullscreen: bool,
    #[serde(default = "legacy_onboarding_completed")]
    pub onboarding_completed: bool,
    #[serde(default = "legacy_enabled_providers")]
    pub enabled_providers: Vec<ProviderId>,
    #[serde(default = "legacy_provider_setup_version")]
    pub provider_setup_version: u16,
}

impl AppSettings {
    pub fn requires_provider_setup(&self) -> bool {
        !self.onboarding_completed || self.provider_setup_version < CURRENT_PROVIDER_SETUP_VERSION
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            placement: EdgePlacement::Right,
            monitor: None,
            locale: LocaleCode::En,
            always_show_over_fullscreen: false,
            onboarding_completed: false,
            enabled_providers: Vec::new(),
            provider_setup_version: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<EdgePlacement>,
    #[serde(
        default,
        deserialize_with = "deserialize_nullable_monitor",
        skip_serializing_if = "Option::is_none"
    )]
    pub monitor: Option<Option<MonitorPreference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<LocaleCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_show_over_fullscreen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboarding_completed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_providers: Option<Vec<ProviderId>>,
    #[serde(skip)]
    pub(crate) provider_setup_version: Option<u16>,
}

fn deserialize_nullable_monitor<'de, D>(
    deserializer: D,
) -> Result<Option<Option<MonitorPreference>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<MonitorPreference>::deserialize(deserializer).map(Some)
}

pub trait SettingsPersistence: Send + Sync {
    fn load(&self) -> Result<Option<Value>, String>;
    fn save(&self, settings: &AppSettings) -> Result<(), String>;
}

trait SettingsStoreCache: Send + Sync {
    fn get(&self) -> Option<Value>;
    fn set(&self, value: Value);
    fn delete(&self);
    fn save(&self) -> Result<(), String>;
}

fn persist_settings_transactionally(
    store: &impl SettingsStoreCache,
    next: Value,
) -> Result<(), String> {
    let previous = store.get();
    store.set(next);
    if let Err(error) = store.save() {
        match previous {
            Some(value) => store.set(value),
            None => store.delete(),
        }
        return Err(error);
    }
    Ok(())
}

pub struct SettingsService {
    persistence: Arc<dyn SettingsPersistence>,
    settings: RwLock<AppSettings>,
    changes: watch::Sender<AppSettings>,
}

impl SettingsService {
    pub fn load(persistence: Arc<dyn SettingsPersistence>) -> Self {
        let settings = persistence
            .load()
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_value::<AppSettings>(value).ok())
            .filter(|settings| validate_settings(settings).is_ok())
            .unwrap_or_default();
        let (changes, _) = watch::channel(settings.clone());

        Self {
            persistence,
            settings: RwLock::new(settings),
            changes,
        }
    }

    pub fn current(&self) -> Result<AppSettings, String> {
        self.settings
            .read()
            .map(|settings| settings.clone())
            .map_err(|_| "settings lock poisoned".to_string())
    }

    pub fn subscribe(&self) -> watch::Receiver<AppSettings> {
        self.changes.subscribe()
    }

    pub fn update(&self, patch: SettingsPatch) -> Result<AppSettings, String> {
        let mut settings = self
            .settings
            .write()
            .map_err(|_| "settings lock poisoned".to_string())?;
        let mut next = settings.clone();

        if let Some(placement) = patch.placement {
            next.placement = placement;
        }
        if let Some(monitor) = patch.monitor {
            next.monitor = monitor;
        }
        if let Some(locale) = patch.locale {
            next.locale = locale;
        }
        if let Some(always_show_over_fullscreen) = patch.always_show_over_fullscreen {
            next.always_show_over_fullscreen = always_show_over_fullscreen;
        }
        if let Some(onboarding_completed) = patch.onboarding_completed {
            next.onboarding_completed = onboarding_completed;
        }
        if let Some(enabled_providers) = patch.enabled_providers {
            next.enabled_providers = enabled_providers;
        }
        if let Some(provider_setup_version) = patch.provider_setup_version {
            next.provider_setup_version = provider_setup_version;
        }

        validate_settings(&next)?;
        self.persistence.save(&next)?;
        *settings = next.clone();
        self.changes.send_replace(next.clone());
        Ok(next)
    }
}

pub fn service_from_tauri_store<R, M>(manager: &M) -> Result<SettingsService, String>
where
    R: Runtime,
    M: Manager<R>,
{
    let store = manager
        .store_builder(SETTINGS_STORE_FILE)
        .disable_auto_save()
        .build()
        .map_err(|error| format!("failed to open settings store: {error}"))?;
    Ok(SettingsService::load(Arc::new(
        TauriStoreSettingsPersistence { store },
    )))
}

struct TauriStoreSettingsPersistence<R: Runtime> {
    store: Arc<tauri_plugin_store::Store<R>>,
}

impl<R: Runtime> SettingsPersistence for TauriStoreSettingsPersistence<R> {
    fn load(&self) -> Result<Option<Value>, String> {
        Ok(self.store.get(SETTINGS_STORE_KEY))
    }

    fn save(&self, settings: &AppSettings) -> Result<(), String> {
        let value = serde_json::to_value(settings)
            .map_err(|error| format!("failed to serialize settings: {error}"))?;
        persist_settings_transactionally(self, value)
    }
}

impl<R: Runtime> SettingsStoreCache for TauriStoreSettingsPersistence<R> {
    fn get(&self) -> Option<Value> {
        self.store.get(SETTINGS_STORE_KEY)
    }

    fn set(&self, value: Value) {
        self.store.set(SETTINGS_STORE_KEY, value);
    }

    fn delete(&self) {
        self.store.delete(SETTINGS_STORE_KEY);
    }

    fn save(&self) -> Result<(), String> {
        self.store
            .save()
            .map_err(|error| format!("failed to save settings: {error}"))
    }
}

fn validate_settings(settings: &AppSettings) -> Result<(), String> {
    if let Some(monitor) = &settings.monitor {
        validate_monitor_preference(monitor)?;
    }
    let unique = settings
        .enabled_providers
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if unique.len() != settings.enabled_providers.len() {
        return Err("enabled providers must be unique".into());
    }
    Ok(())
}

fn validate_monitor_preference(monitor: &MonitorPreference) -> Result<(), String> {
    validate_monitor_id(&monitor.id)?;
    validate_monitor_text("monitor name", &monitor.name)?;

    let work_area = &monitor.last_work_area;
    if work_area.width == 0 || work_area.height == 0 {
        return Err("monitor work area must have non-zero dimensions".into());
    }
    let width = i32::try_from(work_area.width)
        .map_err(|_| "monitor work area width is too large".to_string())?;
    let height = i32::try_from(work_area.height)
        .map_err(|_| "monitor work area height is too large".to_string())?;
    work_area
        .x
        .checked_add(width)
        .ok_or_else(|| "monitor work area horizontal bounds overflow".to_string())?;
    work_area
        .y
        .checked_add(height)
        .ok_or_else(|| "monitor work area vertical bounds overflow".to_string())?;
    Ok(())
}

fn validate_monitor_id(value: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().count() > MAX_MONITOR_TEXT_LENGTH {
        return Err(format!(
            "monitor id must contain 1 to {MAX_MONITOR_TEXT_LENGTH} characters"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err("monitor id contains unsafe characters".into());
    }
    Ok(())
}

fn validate_monitor_text(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().count() > MAX_MONITOR_TEXT_LENGTH {
        return Err(format!(
            "{label} must contain 1 to {MAX_MONITOR_TEXT_LENGTH} characters"
        ));
    }
    if value
        .chars()
        .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(format!("{label} contains unsafe characters"));
    }
    Ok(())
}

#[cfg(test)]
#[derive(Default)]
struct MemorySettingsPersistence {
    value: RwLock<Option<Value>>,
}

#[cfg(test)]
impl MemorySettingsPersistence {
    fn with_raw(raw: &str) -> Self {
        Self {
            value: RwLock::new(Some(Value::String(raw.into()))),
        }
    }
}

#[cfg(test)]
impl SettingsPersistence for MemorySettingsPersistence {
    fn load(&self) -> Result<Option<Value>, String> {
        Ok(self
            .value
            .read()
            .expect("memory settings lock poisoned")
            .clone())
    }

    fn save(&self, settings: &AppSettings) -> Result<(), String> {
        *self.value.write().expect("memory settings lock poisoned") =
            Some(serde_json::to_value(settings).expect("settings serialization must succeed"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::models::ProviderId;
    use std::sync::{Arc, Mutex};

    struct SaveFailingStore {
        cached: Mutex<Option<Value>>,
        durable: Mutex<Option<Value>>,
        auto_save_enabled: bool,
        scheduled_save: Mutex<bool>,
        later_durable_mutation_hooks: Mutex<usize>,
    }

    impl SaveFailingStore {
        fn new(initial: Value) -> Self {
            Self {
                cached: Mutex::new(Some(initial.clone())),
                durable: Mutex::new(Some(initial)),
                auto_save_enabled: false,
                scheduled_save: Mutex::new(false),
                later_durable_mutation_hooks: Mutex::new(0),
            }
        }

        fn cached(&self) -> Option<Value> {
            self.cached.lock().unwrap().clone()
        }

        fn durable(&self) -> Option<Value> {
            self.durable.lock().unwrap().clone()
        }

        fn run_later_durable_mutation_hook(&self) {
            if std::mem::take(&mut *self.scheduled_save.lock().unwrap()) {
                *self.later_durable_mutation_hooks.lock().unwrap() += 1;
                *self.durable.lock().unwrap() = self.cached();
            }
        }
    }

    impl SettingsStoreCache for SaveFailingStore {
        fn get(&self) -> Option<Value> {
            self.cached()
        }

        fn set(&self, value: Value) {
            *self.cached.lock().unwrap() = Some(value);
            if self.auto_save_enabled {
                *self.scheduled_save.lock().unwrap() = true;
            }
        }

        fn delete(&self) {
            *self.cached.lock().unwrap() = None;
            if self.auto_save_enabled {
                *self.scheduled_save.lock().unwrap() = true;
            }
        }

        fn save(&self) -> Result<(), String> {
            Err("simulated save failure".into())
        }
    }

    struct FailingPersistence {
        store: Arc<SaveFailingStore>,
    }

    impl SettingsPersistence for FailingPersistence {
        fn load(&self) -> Result<Option<Value>, String> {
            Ok(self.store.cached())
        }

        fn save(&self, settings: &AppSettings) -> Result<(), String> {
            persist_settings_transactionally(
                self.store.as_ref(),
                serde_json::to_value(settings).unwrap(),
            )
        }
    }

    fn monitor() -> MonitorPreference {
        MonitorPreference {
            id: "display-a".into(),
            name: "Desk display".into(),
            last_work_area: StoredMonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            },
        }
    }

    #[test]
    fn defaults_are_english_right_primary_and_hide_over_fullscreen() {
        assert_eq!(
            AppSettings::default(),
            AppSettings {
                placement: EdgePlacement::Right,
                monitor: None,
                locale: LocaleCode::En,
                always_show_over_fullscreen: false,
                onboarding_completed: false,
                enabled_providers: Vec::new(),
                provider_setup_version: 0,
            }
        );
    }

    #[test]
    fn clean_install_requires_onboarding_and_enables_nothing() {
        let settings = AppSettings::default();
        assert!(!settings.onboarding_completed);
        assert!(settings.enabled_providers.is_empty());
        assert!(settings.requires_provider_setup());
    }

    #[test]
    fn legacy_settings_keep_provider_choices_but_require_one_current_setup_review() {
        let legacy = serde_json::json!({
            "placement": "right",
            "monitor": null,
            "locale": "en",
            "alwaysShowOverFullscreen": false
        });
        let migrated: AppSettings = serde_json::from_value(legacy).unwrap();
        assert!(migrated.onboarding_completed);
        // Legacy files predate grok/cursor; migration keeps exactly the era's trio.
        assert_eq!(
            migrated.enabled_providers,
            vec![ProviderId::Claude, ProviderId::Codex, ProviderId::GitHub]
        );
        assert_eq!(migrated.provider_setup_version, 0);
        assert!(migrated.requires_provider_setup());
    }

    #[test]
    fn immediately_previous_provider_setup_version_requires_one_current_review() {
        let settings = AppSettings {
            onboarding_completed: true,
            provider_setup_version: CURRENT_PROVIDER_SETUP_VERSION - 1,
            enabled_providers: ProviderId::ALL.to_vec(),
            ..Default::default()
        };

        assert!(settings.requires_provider_setup());
    }

    #[test]
    fn current_provider_setup_version_does_not_reprompt() {
        let settings = AppSettings {
            onboarding_completed: true,
            provider_setup_version: CURRENT_PROVIDER_SETUP_VERSION,
            enabled_providers: vec![ProviderId::Claude],
            ..Default::default()
        };

        assert!(!settings.requires_provider_setup());
    }

    #[test]
    fn completing_provider_setup_persists_the_current_version_and_exact_selection() {
        let persistence = Arc::new(MemorySettingsPersistence::default());
        let service = SettingsService::load(persistence);

        let completed = service
            .update(SettingsPatch {
                onboarding_completed: Some(true),
                enabled_providers: Some(vec![ProviderId::Codex]),
                provider_setup_version: Some(CURRENT_PROVIDER_SETUP_VERSION),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(completed.enabled_providers, vec![ProviderId::Codex]);
        assert_eq!(
            completed.provider_setup_version,
            CURRENT_PROVIDER_SETUP_VERSION
        );
        assert!(!completed.requires_provider_setup());
    }

    #[test]
    fn rejects_duplicate_enabled_providers() {
        let persistence = Arc::new(MemorySettingsPersistence::default());
        let service = SettingsService::load(persistence);
        let error = service.update(SettingsPatch {
            enabled_providers: Some(vec![ProviderId::Claude, ProviderId::Claude]),
            ..Default::default()
        });
        assert_eq!(error.unwrap_err(), "enabled providers must be unique");
    }

    #[test]
    fn accepts_exactly_the_eight_supported_locale_codes() {
        for code in ["en", "he", "ar", "es", "ru", "fr", "zh-CN", "ja"] {
            let locale: LocaleCode = serde_json::from_str(&format!("\"{code}\"")).unwrap();
            assert_eq!(
                serde_json::to_string(&locale).unwrap(),
                format!("\"{code}\"")
            );
        }
    }

    #[test]
    fn rejects_unknown_locales_and_placements() {
        assert!(serde_json::from_str::<LocaleCode>("\"de\"").is_err());
        assert!(serde_json::from_str::<EdgePlacement>("\"bottom\"").is_err());
    }

    #[tokio::test]
    async fn preserves_an_unavailable_monitor_preference_and_recovery_metadata() {
        let persistence = Arc::new(MemorySettingsPersistence::default());
        let service = SettingsService::load(persistence);
        let saved = service
            .update(SettingsPatch {
                monitor: Some(Some(monitor())),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(saved.monitor, Some(monitor()));
        assert_eq!(service.current().unwrap().monitor, Some(monitor()));
    }

    #[tokio::test]
    async fn falls_back_to_defaults_when_persisted_content_is_malformed() {
        let persistence = Arc::new(MemorySettingsPersistence::with_raw("not settings"));
        let service = SettingsService::load(persistence);

        assert_eq!(service.current().unwrap(), AppSettings::default());
    }

    #[tokio::test]
    async fn sends_one_notification_for_each_successful_update() {
        let persistence = Arc::new(MemorySettingsPersistence::default());
        let service = SettingsService::load(persistence);
        let mut changes = service.subscribe();

        let updated = service
            .update(SettingsPatch {
                placement: Some(EdgePlacement::Left),
                ..Default::default()
            })
            .unwrap();

        changes.changed().await.unwrap();
        assert_eq!(*changes.borrow_and_update(), updated);
        assert!(!changes.has_changed().unwrap());
    }

    #[tokio::test]
    async fn failed_persistence_restores_store_cache_without_publishing_or_later_flush() {
        let original = serde_json::to_value(AppSettings::default()).unwrap();
        let store = Arc::new(SaveFailingStore::new(original.clone()));
        let service = SettingsService::load(Arc::new(FailingPersistence {
            store: store.clone(),
        }));
        let changes = service.subscribe();

        assert!(service
            .update(SettingsPatch {
                placement: Some(EdgePlacement::Left),
                ..Default::default()
            })
            .is_err());

        assert_eq!(store.cached(), Some(original.clone()));
        store.run_later_durable_mutation_hook();
        assert_eq!(store.durable(), Some(original));
        assert_eq!(service.current().unwrap(), AppSettings::default());
        assert_eq!(*store.later_durable_mutation_hooks.lock().unwrap(), 0);
        assert!(!changes.has_changed().unwrap());
    }

    #[test]
    fn poisoned_settings_lock_returns_a_recoverable_error() {
        let service = Arc::new(SettingsService::load(Arc::new(
            MemorySettingsPersistence::default(),
        )));
        let poisoned_service = service.clone();

        let _ = std::thread::spawn(move || {
            let _guard = poisoned_service.settings.write().unwrap();
            panic!("poison settings lock");
        })
        .join();

        assert_eq!(service.current().unwrap_err(), "settings lock poisoned");
    }

    #[test]
    fn distinguishes_an_omitted_monitor_patch_from_a_null_monitor_patch() {
        let omitted: SettingsPatch = serde_json::from_str("{}").unwrap();
        let clear: SettingsPatch = serde_json::from_str(r#"{"monitor":null}"#).unwrap();

        assert_eq!(omitted.monitor, None);
        assert_eq!(clear.monitor, Some(None));
    }

    #[tokio::test]
    async fn rejects_invalid_monitor_preferences() {
        let persistence = Arc::new(MemorySettingsPersistence::default());
        let service = SettingsService::load(persistence);
        let invalid = MonitorPreference {
            id: "bad/name".into(),
            name: "Desk display".into(),
            last_work_area: StoredMonitorRect {
                x: i32::MAX,
                y: 0,
                width: 1,
                height: 100,
            },
        };

        assert!(service
            .update(SettingsPatch {
                monitor: Some(Some(invalid)),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn accepts_the_native_windows_display_identifier_format() {
        let mut native = monitor();
        native.id = r"\\.\DISPLAY1".into();

        assert!(validate_monitor_preference(&native).is_ok());
    }
}
