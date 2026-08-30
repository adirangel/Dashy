use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use chrono::{DateTime, Duration, Utc};
use tokio::sync::{Mutex, RwLock};

use crate::dashboard::{
    models::{
        DashboardSnapshot, GitHubData, GitHubSnapshot, ProviderError, ProviderErrorKind,
        ProviderId, ProviderStatus, UsageData, UsageSnapshot,
    },
    providers::DataProvider,
};

const CACHE_TTL: Duration = Duration::minutes(5);

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

pub struct DashboardService {
    github: Arc<dyn DataProvider<GitHubData>>,
    codex: Arc<dyn DataProvider<UsageData>>,
    claude: Arc<dyn DataProvider<UsageData>>,
    clock: Arc<dyn Clock>,
    cache: RwLock<Option<DashboardSnapshot>>,
    refresh_lock: Mutex<()>,
    refresh_generation: AtomicU64,
    github_refresh_gate: Mutex<()>,
    codex_refresh_gate: Mutex<()>,
    claude_refresh_gate: Mutex<()>,
    github_generation: AtomicU64,
    codex_generation: AtomicU64,
    claude_generation: AtomicU64,
}

impl DashboardService {
    pub fn new<G, C, L, K>(github: Arc<G>, codex: Arc<C>, claude: Arc<L>, clock: Arc<K>) -> Self
    where
        G: DataProvider<GitHubData> + 'static,
        C: DataProvider<UsageData> + 'static,
        L: DataProvider<UsageData> + 'static,
        K: Clock + 'static,
    {
        Self {
            github,
            codex,
            claude,
            clock,
            cache: RwLock::new(None),
            refresh_lock: Mutex::new(()),
            refresh_generation: AtomicU64::new(0),
            github_refresh_gate: Mutex::new(()),
            codex_refresh_gate: Mutex::new(()),
            claude_refresh_gate: Mutex::new(()),
            github_generation: AtomicU64::new(0),
            codex_generation: AtomicU64::new(0),
            claude_generation: AtomicU64::new(0),
        }
    }

    pub async fn get_snapshot(&self, force: bool) -> DashboardSnapshot {
        let observed_generation = self.refresh_generation.load(Ordering::Acquire);
        if !force {
            if let Some(snapshot) = self.fresh_snapshot().await {
                return snapshot;
            }
        }

        let _refresh_guard = self.refresh_lock.lock().await;

        if force && self.refresh_generation.load(Ordering::Acquire) > observed_generation {
            return self
                .cache
                .read()
                .await
                .clone()
                .expect("a completed refresh must store a snapshot");
        }
        if !force {
            if let Some(snapshot) = self.fresh_snapshot().await {
                return snapshot;
            }
        }

        let github_generation = self.github_generation.load(Ordering::Acquire);
        let codex_generation = self.codex_generation.load(Ordering::Acquire);
        let claude_generation = self.claude_generation.load(Ordering::Acquire);
        tokio::join!(
            self.refresh_github(github_generation),
            self.refresh_codex(codex_generation),
            self.refresh_claude(claude_generation),
        );
        let snapshot = self.cached_snapshot().await;
        self.refresh_generation.fetch_add(1, Ordering::Release);
        snapshot
    }

    pub async fn refresh_provider(&self, provider: ProviderId) -> DashboardSnapshot {
        let observed_generation = self.provider_generation(provider).load(Ordering::Acquire);
        match provider {
            ProviderId::GitHub => self.refresh_github(observed_generation).await,
            ProviderId::Codex => self.refresh_codex(observed_generation).await,
            ProviderId::Claude => self.refresh_claude(observed_generation).await,
        }
        self.cached_snapshot().await
    }

    async fn fresh_snapshot(&self) -> Option<DashboardSnapshot> {
        let snapshot = self.cache.read().await.clone()?;
        (self
            .clock
            .now()
            .signed_duration_since(snapshot.refreshed_at)
            < CACHE_TTL)
            .then_some(snapshot)
    }

    fn provider_generation(&self, provider: ProviderId) -> &AtomicU64 {
        match provider {
            ProviderId::GitHub => &self.github_generation,
            ProviderId::Codex => &self.codex_generation,
            ProviderId::Claude => &self.claude_generation,
        }
    }

    async fn refresh_github(&self, observed_generation: u64) {
        let _provider_guard = self.github_refresh_gate.lock().await;
        if self.github_generation.load(Ordering::Acquire) > observed_generation {
            return;
        }

        let result = self.github.fetch().await;
        let refreshed_at = self.clock.now();
        let mut cache = self.cache.write().await;
        let previous = cache.as_ref().map(|snapshot| snapshot.github.clone());
        let snapshot = cache.get_or_insert_with(|| empty_snapshot(refreshed_at));
        snapshot.github = merge_github(result, previous.as_ref(), refreshed_at);
        snapshot.refreshed_at = snapshot.refreshed_at.max(refreshed_at);
        self.github_generation.fetch_add(1, Ordering::Release);
    }

    async fn refresh_codex(&self, observed_generation: u64) {
        let _provider_guard = self.codex_refresh_gate.lock().await;
        if self.codex_generation.load(Ordering::Acquire) > observed_generation {
            return;
        }

        let result = self.codex.fetch().await;
        let refreshed_at = self.clock.now();
        let mut cache = self.cache.write().await;
        let previous = cache.as_ref().map(|snapshot| snapshot.codex.clone());
        let snapshot = cache.get_or_insert_with(|| empty_snapshot(refreshed_at));
        snapshot.codex = merge_usage(result, previous.as_ref(), refreshed_at);
        snapshot.refreshed_at = snapshot.refreshed_at.max(refreshed_at);
        self.codex_generation.fetch_add(1, Ordering::Release);
    }

    async fn refresh_claude(&self, observed_generation: u64) {
        let _provider_guard = self.claude_refresh_gate.lock().await;
        if self.claude_generation.load(Ordering::Acquire) > observed_generation {
            return;
        }

        let result = self.claude.fetch().await;
        let refreshed_at = self.clock.now();
        let mut cache = self.cache.write().await;
        let previous = cache.as_ref().map(|snapshot| snapshot.claude.clone());
        let snapshot = cache.get_or_insert_with(|| empty_snapshot(refreshed_at));
        snapshot.claude = merge_usage(result, previous.as_ref(), refreshed_at);
        snapshot.refreshed_at = snapshot.refreshed_at.max(refreshed_at);
        self.claude_generation.fetch_add(1, Ordering::Release);
    }

    async fn cached_snapshot(&self) -> DashboardSnapshot {
        self.cache
            .read()
            .await
            .clone()
            .expect("a completed provider refresh must store a snapshot")
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
        refreshed_at,
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
            ContributionDay, GitHubData, ProviderError, ProviderErrorKind, ProviderId,
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
            let service = Arc::new(DashboardService::new(
                github.clone(),
                codex.clone(),
                claude.clone(),
                clock.clone(),
            ));
            Self {
                service,
                clock,
                github,
                codex,
                claude,
            }
        }
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
