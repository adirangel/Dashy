use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use chrono::{DateTime, Duration, Utc};
use tokio::sync::{Mutex, RwLock};

use crate::dashboard::{
    models::{
        AccountData, AccountSnapshot, DashboardSnapshot, GitHubData, GitHubSnapshot, ProviderError,
        ProviderErrorKind, ProviderId, ProviderStatus, UsageData, UsageSnapshot,
    },
    providers::DataProvider,
};

const CACHE_TTL: Duration = Duration::minutes(5);
const PROVIDER_COUNT: usize = ProviderId::ALL.len();

fn provider_index(provider: ProviderId) -> usize {
    match provider {
        ProviderId::Claude => 0,
        ProviderId::Codex => 1,
        ProviderId::GitHub => 2,
        ProviderId::Grok => 3,
        ProviderId::Cursor => 4,
    }
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

struct ProviderSlot {
    gate: Mutex<()>,
    generation: AtomicU64,
}

pub struct DashboardService {
    github: Arc<dyn DataProvider<GitHubData>>,
    codex: Arc<dyn DataProvider<UsageData>>,
    claude: Arc<dyn DataProvider<UsageData>>,
    grok: Arc<dyn DataProvider<UsageData>>,
    cursor: Arc<dyn DataProvider<AccountData>>,
    clock: Arc<dyn Clock>,
    cache: RwLock<Option<DashboardSnapshot>>,
    provider_refreshed_at: RwLock<[Option<DateTime<Utc>>; PROVIDER_COUNT]>,
    slots: [ProviderSlot; PROVIDER_COUNT],
}

impl DashboardService {
    pub fn new<G, C, L, X, U, K>(
        github: Arc<G>,
        codex: Arc<C>,
        claude: Arc<L>,
        grok: Arc<X>,
        cursor: Arc<U>,
        clock: Arc<K>,
    ) -> Self
    where
        G: DataProvider<GitHubData> + 'static,
        C: DataProvider<UsageData> + 'static,
        L: DataProvider<UsageData> + 'static,
        X: DataProvider<UsageData> + 'static,
        U: DataProvider<AccountData> + 'static,
        K: Clock + 'static,
    {
        Self {
            github,
            codex,
            claude,
            grok,
            cursor,
            clock,
            cache: RwLock::new(None),
            provider_refreshed_at: RwLock::new([None; PROVIDER_COUNT]),
            slots: std::array::from_fn(|_| ProviderSlot {
                gate: Mutex::new(()),
                generation: AtomicU64::new(0),
            }),
        }
    }

    pub async fn get_snapshot(&self, force: bool) -> DashboardSnapshot {
        self.get_snapshot_for(force, &ProviderId::ALL).await
    }

    pub async fn get_snapshot_for(
        &self,
        force: bool,
        providers: &[ProviderId],
    ) -> DashboardSnapshot {
        if providers.is_empty() {
            return self.cached_snapshot_or_empty().await;
        }
        let now = self.clock.now();
        let freshness = self.provider_refreshed_at.read().await;
        let mut wanted = [false; PROVIDER_COUNT];
        for provider in ProviderId::ALL {
            let index = provider_index(provider);
            wanted[index] = providers.contains(&provider)
                && (force
                    || freshness[index]
                        .is_none_or(|at| now.signed_duration_since(at) >= CACHE_TTL));
        }
        drop(freshness);

        let observed: [u64; PROVIDER_COUNT] =
            std::array::from_fn(|index| self.slots[index].generation.load(Ordering::Acquire));
        tokio::join!(
            self.refresh_when_wanted(ProviderId::Claude, &wanted, &observed),
            self.refresh_when_wanted(ProviderId::Codex, &wanted, &observed),
            self.refresh_when_wanted(ProviderId::GitHub, &wanted, &observed),
            self.refresh_when_wanted(ProviderId::Grok, &wanted, &observed),
            self.refresh_when_wanted(ProviderId::Cursor, &wanted, &observed),
        );
        self.cached_snapshot_or_empty().await
    }

    async fn refresh_when_wanted(
        &self,
        provider: ProviderId,
        wanted: &[bool; PROVIDER_COUNT],
        observed: &[u64; PROVIDER_COUNT],
    ) {
        let index = provider_index(provider);
        if wanted[index] {
            self.refresh_slot(provider, Some(observed[index])).await;
        }
    }

    pub async fn refresh_provider(&self, provider: ProviderId) -> DashboardSnapshot {
        let observed_generation = self.slots[provider_index(provider)]
            .generation
            .load(Ordering::Acquire);
        self.refresh_slot(provider, Some(observed_generation)).await;
        self.cached_snapshot_or_empty().await
    }

    /// Reconciles provider state after an external setup process has exited.
    ///
    /// Unlike ordinary refreshes, this deliberately does not coalesce with a
    /// request that acquired the provider gate before the mutation completed:
    /// once that older request releases the gate, we fetch the post-mutation
    /// state ourselves.
    pub async fn refresh_provider_after_mutation(&self, provider: ProviderId) -> DashboardSnapshot {
        self.refresh_slot(provider, None).await;
        self.cached_snapshot_or_empty().await
    }

    async fn refresh_slot(&self, provider: ProviderId, observed_generation: Option<u64>) {
        let slot = &self.slots[provider_index(provider)];
        let _provider_guard = slot.gate.lock().await;
        if let Some(observed) = observed_generation {
            if slot.generation.load(Ordering::Acquire) > observed {
                return;
            }
        }

        self.fetch_and_publish(provider).await;
    }

    async fn fetch_and_publish(&self, provider: ProviderId) {
        match provider {
            ProviderId::GitHub => {
                let result = self.github.fetch().await;
                self.publish_with(
                    provider,
                    |snapshot| &mut snapshot.github,
                    |previous, at| merge_github(result, previous, at),
                )
                .await;
            }
            ProviderId::Codex => {
                let result = self.codex.fetch().await;
                self.publish_with(
                    provider,
                    |snapshot| &mut snapshot.codex,
                    |previous, at| merge_usage(result, previous, at),
                )
                .await;
            }
            ProviderId::Claude => {
                let result = self.claude.fetch().await;
                self.publish_with(
                    provider,
                    |snapshot| &mut snapshot.claude,
                    |previous, at| merge_usage(result, previous, at),
                )
                .await;
            }
            ProviderId::Grok => {
                let result = self.grok.fetch().await;
                self.publish_with(
                    provider,
                    |snapshot| &mut snapshot.grok,
                    |previous, at| merge_usage(result, previous, at),
                )
                .await;
            }
            ProviderId::Cursor => {
                let result = self.cursor.fetch().await;
                self.publish_with(
                    provider,
                    |snapshot| &mut snapshot.cursor,
                    |previous, at| merge_account(result, previous, at),
                )
                .await;
            }
        }
    }

    // The empty snapshot's field stands in for "no previous data" on first load;
    // every stale_* helper bails on its `last_successful_refresh: None`, so this is
    // behavior-identical to the pre-slot code that passed `None` there.
    async fn publish_with<S: Clone>(
        &self,
        provider: ProviderId,
        field: impl Fn(&mut DashboardSnapshot) -> &mut S,
        merge: impl FnOnce(Option<&S>, DateTime<Utc>) -> S,
    ) {
        let refreshed_at = self.clock.now();
        let mut cache = self.cache.write().await;
        let snapshot = cache.get_or_insert_with(|| empty_snapshot(refreshed_at));
        let previous = field(snapshot).clone();
        *field(snapshot) = merge(Some(&previous), refreshed_at);
        snapshot.refreshed_at = snapshot.refreshed_at.max(refreshed_at);
        self.provider_refreshed_at.write().await[provider_index(provider)] = Some(refreshed_at);
        self.slots[provider_index(provider)]
            .generation
            .fetch_add(1, Ordering::Release);
    }

    async fn cached_snapshot_or_empty(&self) -> DashboardSnapshot {
        self.cache
            .read()
            .await
            .clone()
            .unwrap_or_else(|| empty_snapshot(self.clock.now()))
    }
}

fn empty_snapshot(refreshed_at: DateTime<Utc>) -> DashboardSnapshot {
    DashboardSnapshot {
        github: GitHubSnapshot {
            status: ProviderStatus::Unavailable,
            account_login: None,
            contribution_days: None,
            current_streak_days: None,
            last_successful_refresh: None,
            error_kind: None,
        },
        codex: empty_usage_snapshot(),
        claude: empty_usage_snapshot(),
        grok: empty_usage_snapshot(),
        cursor: empty_account_snapshot(),
        refreshed_at,
    }
}

fn empty_account_snapshot() -> AccountSnapshot {
    AccountSnapshot {
        status: ProviderStatus::Unavailable,
        subscription_tier: None,
        account_email: None,
        last_successful_refresh: None,
        error_kind: None,
    }
}

fn empty_usage_snapshot() -> UsageSnapshot {
    UsageSnapshot {
        status: ProviderStatus::Unavailable,
        remaining_percent: None,
        short_window: None,
        weekly_window: None,
        last_successful_refresh: None,
        error_kind: None,
    }
}

fn merge_github(
    result: Result<GitHubData, ProviderError>,
    previous: Option<&GitHubSnapshot>,
    refreshed_at: DateTime<Utc>,
) -> GitHubSnapshot {
    match result {
        Ok(data) => GitHubSnapshot::connected(data, refreshed_at),
        Err(error) => stale_github(previous, &error).unwrap_or_else(|| {
            let (status, error_kind) = map_error(&error);
            GitHubSnapshot::failed(status, error_kind)
        }),
    }
}

fn merge_usage(
    result: Result<UsageData, ProviderError>,
    previous: Option<&UsageSnapshot>,
    refreshed_at: DateTime<Utc>,
) -> UsageSnapshot {
    match result {
        Ok(data) => UsageSnapshot::connected(data, refreshed_at),
        Err(error) => stale_usage(previous, &error).unwrap_or_else(|| {
            let (status, error_kind) = map_error(&error);
            UsageSnapshot::failed(status, error_kind)
        }),
    }
}

fn stale_github(
    previous: Option<&GitHubSnapshot>,
    error: &ProviderError,
) -> Option<GitHubSnapshot> {
    let previous = previous?;
    previous.account_login.as_ref()?;
    previous.contribution_days.as_ref()?;
    previous.current_streak_days?;
    previous.last_successful_refresh?;
    let (_, error_kind) = map_error(error);
    let mut stale = previous.clone();
    stale.status = ProviderStatus::Stale;
    stale.error_kind = Some(error_kind);
    Some(stale)
}

fn stale_usage(previous: Option<&UsageSnapshot>, error: &ProviderError) -> Option<UsageSnapshot> {
    let previous = previous?;
    previous.remaining_percent?;
    previous.last_successful_refresh?;
    let (_, error_kind) = map_error(error);
    let mut stale = previous.clone();
    stale.status = ProviderStatus::Stale;
    stale.error_kind = Some(error_kind);
    Some(stale)
}

fn merge_account(
    result: Result<AccountData, ProviderError>,
    previous: Option<&AccountSnapshot>,
    refreshed_at: DateTime<Utc>,
) -> AccountSnapshot {
    match result {
        Ok(data) => AccountSnapshot::connected(data, refreshed_at),
        Err(error) => stale_account(previous, &error).unwrap_or_else(|| {
            let (status, error_kind) = map_error(&error);
            AccountSnapshot::failed(status, error_kind)
        }),
    }
}

// Unlike stale_usage, only a prior successful refresh gates staleness: tier and
// email are legitimately absent while connected, so they cannot be required here.
fn stale_account(
    previous: Option<&AccountSnapshot>,
    error: &ProviderError,
) -> Option<AccountSnapshot> {
    let previous = previous?;
    previous.last_successful_refresh?;
    let (_, error_kind) = map_error(error);
    let mut stale = previous.clone();
    stale.status = ProviderStatus::Stale;
    stale.error_kind = Some(error_kind);
    Some(stale)
}

fn map_error(error: &ProviderError) -> (ProviderStatus, ProviderErrorKind) {
    match error {
        ProviderError::NotInstalled => (
            ProviderStatus::NotInstalled,
            ProviderErrorKind::MissingExecutable,
        ),
        ProviderError::NotAuthenticated => (
            ProviderStatus::NotAuthenticated,
            ProviderErrorKind::Authentication,
        ),
        ProviderError::Timeout => (ProviderStatus::Unavailable, ProviderErrorKind::Timeout),
        ProviderError::UnsupportedOutput => (
            ProviderStatus::Unavailable,
            ProviderErrorKind::UnsupportedOutput,
        ),
        ProviderError::Network => (ProviderStatus::Unavailable, ProviderErrorKind::Network),
        ProviderError::Process => (ProviderStatus::Unavailable, ProviderErrorKind::Process),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
    use tokio::sync::{Barrier, Notify};

    use crate::dashboard::{
        models::{
            AccountData, ContributionDay, GitHubData, ProviderError, ProviderErrorKind, ProviderId,
            ProviderStatus, UsageData, UsageWindowData, UsageWindowKind,
        },
        providers::DataProvider,
    };

    use super::{Clock, DashboardService};

    #[derive(Default)]
    struct TestClock {
        now: Mutex<DateTime<Utc>>,
    }

    impl TestClock {
        fn at(value: &str) -> Self {
            Self {
                now: Mutex::new(
                    DateTime::parse_from_rfc3339(value)
                        .unwrap()
                        .with_timezone(&Utc),
                ),
            }
        }

        fn advance_minutes(&self, minutes: i64) {
            *self.now.lock().unwrap() += ChronoDuration::minutes(minutes);
        }

        fn set(&self, value: &str) {
            *self.now.lock().unwrap() = DateTime::parse_from_rfc3339(value)
                .unwrap()
                .with_timezone(&Utc);
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> DateTime<Utc> {
            *self.now.lock().unwrap()
        }
    }

    struct FakeProvider<T> {
        value: Mutex<T>,
        calls: AtomicUsize,
        next_failure: Mutex<Option<ProviderError>>,
        barrier: Mutex<Option<Arc<Barrier>>>,
        started_count: Mutex<Option<Arc<AtomicUsize>>>,
        started_notify: Mutex<Option<Arc<Notify>>>,
        release: Mutex<Option<Arc<Notify>>>,
        blocked_fetches: AtomicUsize,
    }

    impl<T> FakeProvider<T> {
        fn new(value: T) -> Self {
            Self {
                value: Mutex::new(value),
                calls: AtomicUsize::new(0),
                next_failure: Mutex::new(None),
                barrier: Mutex::new(None),
                started_count: Mutex::new(None),
                started_notify: Mutex::new(None),
                release: Mutex::new(None),
                blocked_fetches: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn fail_with(&self, error: ProviderError) {
            *self.next_failure.lock().unwrap() = Some(error);
        }

        fn set_value(&self, value: T) {
            *self.value.lock().unwrap() = value;
        }

        fn wait_at_barrier(&self, barrier: Arc<Barrier>) {
            *self.barrier.lock().unwrap() = Some(barrier);
        }

        fn wait_for_release(
            &self,
            started_count: Arc<AtomicUsize>,
            started_notify: Arc<Notify>,
            release: Arc<Notify>,
        ) {
            *self.started_count.lock().unwrap() = Some(started_count);
            *self.started_notify.lock().unwrap() = Some(started_notify);
            *self.release.lock().unwrap() = Some(release);
            self.blocked_fetches.store(1, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl<T: Clone + Send + Sync + 'static> DataProvider<T> for FakeProvider<T> {
        async fn fetch(&self) -> Result<T, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let barrier = { self.barrier.lock().unwrap().clone() };
            if let Some(barrier) = barrier {
                barrier.wait().await;
            }
            if self
                .blocked_fetches
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                    count.checked_sub(1)
                })
                .is_ok()
            {
                let started_count = { self.started_count.lock().unwrap().clone() };
                let started_notify = { self.started_notify.lock().unwrap().clone() };
                let release = { self.release.lock().unwrap().clone() };
                if let (Some(started_count), Some(started_notify), Some(release)) =
                    (started_count, started_notify, release)
                {
                    started_count.fetch_add(1, Ordering::SeqCst);
                    started_notify.notify_waiters();
                    release.notified().await;
                }
            }
            let next_failure = { self.next_failure.lock().unwrap().take() };
            if let Some(error) = next_failure {
                return Err(error);
            }
            Ok(self.value.lock().unwrap().clone())
        }
    }

    struct ServiceFixture {
        service: Arc<DashboardService>,
        clock: Arc<TestClock>,
        github: Arc<FakeProvider<GitHubData>>,
        codex: Arc<FakeProvider<UsageData>>,
        claude: Arc<FakeProvider<UsageData>>,
        grok: Arc<FakeProvider<UsageData>>,
        cursor: Arc<FakeProvider<AccountData>>,
    }

    impl ServiceFixture {
        fn successful_at(value: &str) -> Self {
            let clock = Arc::new(TestClock::at(value));
            let github = Arc::new(FakeProvider::new(GitHubData {
                account_login: "fixture-user".to_owned(),
                contribution_days: vec![ContributionDay {
                    date: Utc
                        .with_ymd_and_hms(2026, 8, 29, 0, 0, 0)
                        .unwrap()
                        .date_naive(),
                    count: 3,
                    level: 2,
                }],
                current_streak_days: 2,
            }));
            let codex = Arc::new(FakeProvider::new(UsageData {
                short_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Short,
                    remaining_percent: 72,
                    resets_at: None,
                }),
                weekly_window: None,
            }));
            let claude = Arc::new(FakeProvider::new(UsageData {
                short_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Short,
                    remaining_percent: 59,
                    resets_at: None,
                }),
                weekly_window: None,
            }));
            let grok = Arc::new(FakeProvider::new(UsageData {
                short_window: None,
                weekly_window: Some(UsageWindowData {
                    label_key: UsageWindowKind::Monthly,
                    remaining_percent: 85,
                    resets_at: None,
                }),
            }));
            let cursor = Arc::new(FakeProvider::new(AccountData {
                subscription_tier: Some("pro".to_owned()),
                account_email: Some("fixture@example.com".to_owned()),
            }));
            let service = Arc::new(DashboardService::new(
                github.clone(),
                codex.clone(),
                claude.clone(),
                grok.clone(),
                cursor.clone(),
                clock.clone(),
            ));
            Self {
                service,
                clock,
                github,
                codex,
                claude,
                grok,
                cursor,
            }
        }
    }

    #[tokio::test]
    async fn scoped_refresh_never_fetches_disabled_providers() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture
            .service
            .get_snapshot_for(false, &[ProviderId::Claude])
            .await;
        assert_eq!(fixture.claude.calls(), 1);
        assert_eq!(fixture.codex.calls(), 0);
        assert_eq!(fixture.github.calls(), 0);
    }

    #[tokio::test]
    async fn newly_enabled_provider_refreshes_even_when_another_provider_is_fresh() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture
            .service
            .get_snapshot_for(false, &[ProviderId::Claude])
            .await;
        fixture.clock.advance_minutes(1);
        fixture
            .service
            .get_snapshot_for(false, &[ProviderId::Claude, ProviderId::Codex])
            .await;
        assert_eq!(fixture.claude.calls(), 1);
        assert_eq!(fixture.codex.calls(), 1);
        assert_eq!(fixture.github.calls(), 0);
    }

    #[tokio::test]
    async fn empty_provider_scope_returns_without_fetching() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        let snapshot = fixture.service.get_snapshot_for(false, &[]).await;
        assert_eq!(snapshot.claude.status, ProviderStatus::Unavailable);
        assert_eq!(
            fixture.claude.calls() + fixture.codex.calls() + fixture.github.calls(),
            0
        );
    }

    #[tokio::test]
    async fn provider_discovery_fetches_all_five_providers_from_an_empty_cache() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        let snapshot = fixture.service.get_snapshot(true).await;
        assert_eq!(fixture.claude.calls(), 1);
        assert_eq!(fixture.codex.calls(), 1);
        assert_eq!(fixture.github.calls(), 1);
        assert_eq!(fixture.grok.calls(), 1);
        assert_eq!(fixture.cursor.calls(), 1);
        let grok_window = snapshot.grok.weekly_window.unwrap();
        assert_eq!(grok_window.label_key, UsageWindowKind::Monthly);
        assert_eq!(snapshot.grok.remaining_percent, Some(85));
        assert_eq!(snapshot.cursor.status, ProviderStatus::Connected);
        assert_eq!(snapshot.cursor.subscription_tier.as_deref(), Some("pro"));
    }

    #[tokio::test]
    async fn cursor_failure_after_success_retains_the_stale_account() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture
            .service
            .get_snapshot_for(true, &[ProviderId::Cursor])
            .await;
        fixture.cursor.fail_with(ProviderError::Timeout);
        let snapshot = fixture
            .service
            .get_snapshot_for(true, &[ProviderId::Cursor])
            .await;

        assert_eq!(snapshot.cursor.status, ProviderStatus::Stale);
        assert_eq!(snapshot.cursor.subscription_tier.as_deref(), Some("pro"));
        assert_eq!(
            snapshot.cursor.account_email.as_deref(),
            Some("fixture@example.com")
        );
        assert_eq!(snapshot.cursor.error_kind, Some(ProviderErrorKind::Timeout));
    }

    #[tokio::test]
    async fn cursor_failure_without_prior_success_reports_the_failure() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.cursor.fail_with(ProviderError::NotAuthenticated);
        let snapshot = fixture
            .service
            .get_snapshot_for(true, &[ProviderId::Cursor])
            .await;

        assert_eq!(snapshot.cursor.status, ProviderStatus::NotAuthenticated);
        assert!(snapshot.cursor.subscription_tier.is_none());
        assert_eq!(
            snapshot.cursor.error_kind,
            Some(ProviderErrorKind::Authentication)
        );
    }

    #[tokio::test]
    async fn reuses_snapshot_inside_five_minute_ttl() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        fixture.clock.advance_minutes(4);
        fixture.service.get_snapshot(false).await;
        assert_eq!(fixture.github.calls(), 1);
        assert_eq!(fixture.codex.calls(), 1);
        assert_eq!(fixture.claude.calls(), 1);
    }

    #[tokio::test]
    async fn expires_at_five_minutes() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        fixture.clock.advance_minutes(5);
        fixture.service.get_snapshot(false).await;
        assert_eq!(fixture.github.calls(), 2);
        assert_eq!(fixture.codex.calls(), 2);
        assert_eq!(fixture.claude.calls(), 2);
    }

    #[tokio::test]
    async fn force_refresh_bypasses_a_fresh_cache() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        fixture.service.get_snapshot(true).await;
        assert_eq!(fixture.github.calls(), 2);
        assert_eq!(fixture.codex.calls(), 2);
        assert_eq!(fixture.claude.calls(), 2);
    }

    #[tokio::test]
    async fn simultaneous_initial_callers_share_one_refresh() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        let barrier = Arc::new(Barrier::new(3));
        fixture.github.wait_at_barrier(barrier.clone());
        fixture.codex.wait_at_barrier(barrier.clone());
        fixture.claude.wait_at_barrier(barrier);

        let first = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.get_snapshot(false).await }
        });
        let second = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.get_snapshot(false).await }
        });
        let third = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.get_snapshot(false).await }
        });
        first.await.unwrap();
        second.await.unwrap();
        third.await.unwrap();

        assert_eq!(fixture.github.calls(), 1);
        assert_eq!(fixture.codex.calls(), 1);
        assert_eq!(fixture.claude.calls(), 1);
    }

    #[tokio::test]
    async fn concurrent_force_callers_coalesce_with_the_in_flight_generation() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        let started_count = Arc::new(AtomicUsize::new(0));
        let started_notify = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        fixture.github.wait_for_release(
            started_count.clone(),
            started_notify.clone(),
            release.clone(),
        );
        fixture.codex.wait_for_release(
            started_count.clone(),
            started_notify.clone(),
            release.clone(),
        );
        fixture.claude.wait_for_release(
            started_count.clone(),
            started_notify.clone(),
            release.clone(),
        );

        let first = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.get_snapshot(true).await }
        });
        while started_count.load(Ordering::SeqCst) < 3 {
            let notified = started_notify.notified();
            if started_count.load(Ordering::SeqCst) < 3 {
                notified.await;
            }
        }
        let callers = (0..3)
            .map(|_| {
                let service = fixture.service.clone();
                tokio::spawn(async move { service.get_snapshot(true).await })
            })
            .collect::<Vec<_>>();
        tokio::task::yield_now().await;
        release.notify_waiters();
        first.await.unwrap();
        for caller in callers {
            caller.await.unwrap();
        }

        assert_eq!(fixture.github.calls(), 2);
        assert_eq!(fixture.codex.calls(), 2);
        assert_eq!(fixture.claude.calls(), 2);
    }

    #[tokio::test]
    async fn retains_last_good_value_after_one_timeout() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        fixture.clock.advance_minutes(6);
        fixture.claude.fail_with(ProviderError::Timeout);
        let snapshot = fixture.service.get_snapshot(false).await;
        assert_eq!(snapshot.claude.status, ProviderStatus::Stale);
        assert_eq!(snapshot.claude.remaining_percent, Some(59));
        assert_eq!(snapshot.github.status, ProviderStatus::Connected);
    }

    #[tokio::test]
    async fn maps_partial_first_load_failure_to_empty_provider_data() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.github.fail_with(ProviderError::NotInstalled);

        let snapshot = fixture.service.get_snapshot(false).await;

        assert_eq!(snapshot.github.status, ProviderStatus::NotInstalled);
        assert_eq!(
            snapshot.github.error_kind,
            Some(ProviderErrorKind::MissingExecutable)
        );
        assert_eq!(snapshot.github.account_login, None);
        assert_eq!(snapshot.github.contribution_days, None);
        assert_eq!(snapshot.codex.remaining_percent, Some(72));
    }

    #[tokio::test]
    async fn maps_unsupported_output_to_an_unavailable_tile() {
        // The exact chain behind the fresh-machine "Unavailable" bug: a parser
        // rejection must surface as Unavailable with its error kind, not panic
        // or masquerade as another status.
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.claude.fail_with(ProviderError::UnsupportedOutput);

        let snapshot = fixture.service.get_snapshot(false).await;

        assert_eq!(snapshot.claude.status, ProviderStatus::Unavailable);
        assert_eq!(
            snapshot.claude.error_kind,
            Some(ProviderErrorKind::UnsupportedOutput)
        );
        assert_eq!(snapshot.claude.remaining_percent, None);
        assert_eq!(snapshot.claude.short_window, None);
        assert_eq!(snapshot.claude.weekly_window, None);
        assert_eq!(snapshot.codex.status, ProviderStatus::Connected);
    }

    #[tokio::test]
    async fn starts_all_providers_before_any_can_complete() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        let barrier = Arc::new(Barrier::new(3));
        fixture.github.wait_at_barrier(barrier.clone());
        fixture.codex.wait_at_barrier(barrier.clone());
        fixture.claude.wait_at_barrier(barrier);

        tokio::time::timeout(
            Duration::from_millis(250),
            fixture.service.get_snapshot(false),
        )
        .await
        .expect("all providers must start before any completes");
    }

    #[tokio::test]
    async fn selected_refresh_fetches_and_replaces_only_the_requested_provider() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        let before = fixture.service.get_snapshot(false).await;
        fixture.claude.set_value(UsageData {
            short_window: Some(UsageWindowData {
                label_key: UsageWindowKind::Short,
                remaining_percent: 41,
                resets_at: None,
            }),
            weekly_window: None,
        });

        let after = fixture.service.refresh_provider(ProviderId::Claude).await;

        assert_eq!(fixture.github.calls(), 1);
        assert_eq!(fixture.codex.calls(), 1);
        assert_eq!(fixture.claude.calls(), 2);
        assert_eq!(after.github, before.github);
        assert_eq!(after.codex, before.codex);
        assert_eq!(after.claude.remaining_percent, Some(41));
    }

    #[tokio::test]
    async fn simultaneous_selected_refreshes_share_one_provider_fetch() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        let started_count = Arc::new(AtomicUsize::new(0));
        let started_notify = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        fixture.claude.wait_for_release(
            started_count.clone(),
            started_notify.clone(),
            release.clone(),
        );
        let first = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.refresh_provider(ProviderId::Claude).await }
        });
        while started_count.load(Ordering::SeqCst) < 1 {
            let notified = started_notify.notified();
            if started_count.load(Ordering::SeqCst) < 1 {
                notified.await;
            }
        }
        let mut second = Box::pin(fixture.service.refresh_provider(ProviderId::Claude));
        tokio::select! {
            biased;
            _ = &mut second => panic!("the duplicate must wait for the provider gate"),
            _ = async {} => {}
        }
        release.notify_waiters();

        let first_snapshot = first.await.unwrap();
        let second_snapshot = second.await;
        assert_eq!(fixture.github.calls(), 1);
        assert_eq!(fixture.codex.calls(), 1);
        assert_eq!(fixture.claude.calls(), 2);
        assert_eq!(first_snapshot, second_snapshot);
    }

    #[tokio::test]
    async fn post_mutation_refresh_waits_for_an_older_gate_owner_then_always_fetches_again() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        let started_count = Arc::new(AtomicUsize::new(0));
        let started_notify = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        fixture.claude.wait_for_release(
            started_count.clone(),
            started_notify.clone(),
            release.clone(),
        );

        let ordinary = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.refresh_provider(ProviderId::Claude).await }
        });
        while started_count.load(Ordering::SeqCst) < 1 {
            let notified = started_notify.notified();
            if started_count.load(Ordering::SeqCst) < 1 {
                notified.await;
            }
        }

        let post_mutation = tokio::spawn({
            let service = fixture.service.clone();
            async move {
                service
                    .refresh_provider_after_mutation(ProviderId::Claude)
                    .await
            }
        });
        tokio::task::yield_now().await;
        assert_eq!(
            fixture.claude.calls(),
            2,
            "the post-mutation refresh must wait for the current gate owner"
        );

        release.notify_waiters();
        ordinary.await.unwrap();
        post_mutation.await.unwrap();
        assert_eq!(
            fixture.claude.calls(),
            3,
            "the post-mutation refresh must not coalesce with work that began before mutation exit"
        );
    }

    #[tokio::test]
    async fn out_of_order_provider_completions_never_regress_snapshot_refresh_time() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        let started_count = Arc::new(AtomicUsize::new(0));
        let started_notify = Arc::new(Notify::new());
        let codex_release = Arc::new(Notify::new());
        let claude_release = Arc::new(Notify::new());
        fixture.codex.wait_for_release(
            started_count.clone(),
            started_notify.clone(),
            codex_release.clone(),
        );
        fixture.claude.wait_for_release(
            started_count.clone(),
            started_notify.clone(),
            claude_release.clone(),
        );

        let codex = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.refresh_provider(ProviderId::Codex).await }
        });
        let claude = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.refresh_provider(ProviderId::Claude).await }
        });
        while started_count.load(Ordering::SeqCst) < 2 {
            let notified = started_notify.notified();
            if started_count.load(Ordering::SeqCst) < 2 {
                notified.await;
            }
        }

        fixture.clock.set("2026-08-29T09:10:00Z");
        claude_release.notify_waiters();
        let newer = claude.await.unwrap();
        assert_eq!(
            newer.refreshed_at,
            fixture.clock.now(),
            "the first completion records the newer wall-clock timestamp"
        );

        fixture.clock.set("2026-08-29T09:05:00Z");
        codex_release.notify_waiters();
        let final_snapshot = codex.await.unwrap();
        assert_eq!(
            final_snapshot.refreshed_at,
            DateTime::parse_from_rfc3339("2026-08-29T09:10:00Z")
                .unwrap()
                .with_timezone(&Utc),
            "a later completion with an older clock reading must not regress the shared cache"
        );
    }

    #[tokio::test]
    async fn selected_timeout_retains_only_the_requested_providers_stale_value() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        let before = fixture.service.get_snapshot(false).await;
        fixture.clock.advance_minutes(1);
        fixture.claude.fail_with(ProviderError::Timeout);

        let after = fixture.service.refresh_provider(ProviderId::Claude).await;

        assert_eq!(after.github, before.github);
        assert_eq!(after.codex, before.codex);
        assert_eq!(after.claude.status, ProviderStatus::Stale);
        assert_eq!(after.claude.remaining_percent, Some(59));
        assert_eq!(after.claude.error_kind, Some(ProviderErrorKind::Timeout));
        assert_eq!(
            after.claude.last_successful_refresh,
            before.claude.last_successful_refresh
        );
    }

    #[tokio::test]
    async fn selected_refresh_that_owns_the_gate_wins_over_a_racing_full_refresh() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        fixture.service.get_snapshot(false).await;
        fixture.claude.set_value(UsageData {
            short_window: Some(UsageWindowData {
                label_key: UsageWindowKind::Short,
                remaining_percent: 41,
                resets_at: None,
            }),
            weekly_window: None,
        });
        let started_count = Arc::new(AtomicUsize::new(0));
        let started_notify = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        fixture.claude.wait_for_release(
            started_count.clone(),
            started_notify.clone(),
            release.clone(),
        );
        let full_started_count = Arc::new(AtomicUsize::new(0));
        let full_started_notify = Arc::new(Notify::new());
        let full_release = Arc::new(Notify::new());
        fixture.github.wait_for_release(
            full_started_count.clone(),
            full_started_notify.clone(),
            full_release.clone(),
        );

        let selected = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.refresh_provider(ProviderId::Claude).await }
        });
        while started_count.load(Ordering::SeqCst) < 1 {
            let notified = started_notify.notified();
            if started_count.load(Ordering::SeqCst) < 1 {
                notified.await;
            }
        }
        let full = tokio::spawn({
            let service = fixture.service.clone();
            async move { service.get_snapshot(true).await }
        });
        while full_started_count.load(Ordering::SeqCst) < 1 {
            let notified = full_started_notify.notified();
            if full_started_count.load(Ordering::SeqCst) < 1 {
                notified.await;
            }
        }
        release.notify_waiters();

        selected.await.unwrap();
        full_release.notify_waiters();
        let final_snapshot = full.await.unwrap();
        assert_eq!(final_snapshot.claude.remaining_percent, Some(41));
        assert_eq!(fixture.github.calls(), 2);
        assert_eq!(fixture.codex.calls(), 2);
        assert_eq!(fixture.claude.calls(), 2);
    }

    #[tokio::test]
    async fn full_refresh_publishes_successes_when_one_provider_fails() {
        let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
        let before = fixture.service.get_snapshot(false).await;
        fixture.clock.advance_minutes(1);
        fixture.claude.fail_with(ProviderError::Timeout);

        let after = fixture.service.get_snapshot(true).await;

        assert_eq!(after.github.status, ProviderStatus::Connected);
        assert_eq!(after.codex.status, ProviderStatus::Connected);
        assert_eq!(
            after.github.last_successful_refresh,
            Some(fixture.clock.now())
        );
        assert_eq!(
            after.codex.last_successful_refresh,
            Some(fixture.clock.now())
        );
        assert_eq!(after.claude.status, ProviderStatus::Stale);
        assert_eq!(
            after.claude.remaining_percent,
            before.claude.remaining_percent
        );
        assert_eq!(
            after.claude.last_successful_refresh,
            before.claude.last_successful_refresh
        );
    }
}
