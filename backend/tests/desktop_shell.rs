use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use dashy::{
    dashboard::models::ProviderId,
    desktop::{
        controller::{DesktopController, ExitToken, SettingsSource, WindowPort, EXIT_FALLBACK},
        edge::{
            EdgeInteraction, EdgeUiState, EdgeViewState, MonitorWorkArea, Point, Rect, WindowLayout,
        },
        menu::{build_menu_spec, resolve_monitor, MonitorResolution, TrayLabels},
        platform::{DesktopError, DesktopProbe, MonitorDescriptor, NativeWindowHandle},
        settings::{AppSettings, EdgePlacement, MonitorPreference, StoredMonitorRect},
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum WindowAction {
    Apply(WindowLayout),
    Emit(EdgeViewState),
    Focus,
}

#[derive(Default)]
struct FakeWindow {
    actions: Mutex<Vec<WindowAction>>,
    apply_failures_remaining: Mutex<usize>,
    emit_failures_remaining: Mutex<usize>,
    focus_failures_remaining: Mutex<usize>,
}

impl FakeWindow {
    fn actions(&self) -> Vec<WindowAction> {
        self.actions.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.actions.lock().unwrap().clear();
    }

    fn fail_next(&self) {
        self.fail_next_apply();
    }

    fn fail_next_apply(&self) {
        *self.apply_failures_remaining.lock().unwrap() += 1;
    }

    fn fail_next_emit(&self) {
        *self.emit_failures_remaining.lock().unwrap() += 1;
    }

    fn maybe_fail(failures: &Mutex<usize>) -> Result<(), DesktopError> {
        let mut failures = failures.lock().unwrap();
        if *failures == 0 {
            Ok(())
        } else {
            *failures -= 1;
            Err(DesktopError::WindowOperationFailed)
        }
    }
}

impl WindowPort for FakeWindow {
    fn apply(&self, layout: &WindowLayout) -> Result<(), DesktopError> {
        Self::maybe_fail(&self.apply_failures_remaining)?;
        self.actions
            .lock()
            .unwrap()
            .push(WindowAction::Apply(*layout));
        Ok(())
    }

    fn emit_view(&self, state: &EdgeViewState) -> Result<(), DesktopError> {
        Self::maybe_fail(&self.emit_failures_remaining)?;
        self.actions
            .lock()
            .unwrap()
            .push(WindowAction::Emit(*state));
        Ok(())
    }

    fn focus(&self) -> Result<(), DesktopError> {
        Self::maybe_fail(&self.focus_failures_remaining)?;
        self.actions.lock().unwrap().push(WindowAction::Focus);
        Ok(())
    }

    fn native_handles(&self) -> Vec<NativeWindowHandle> {
        vec![41, 42]
    }
}

struct FakeProbe {
    cursor: RwLock<Result<Option<Point>, DesktopError>>,
    monitors: RwLock<Result<Vec<MonitorDescriptor>, DesktopError>>,
    fullscreen: RwLock<bool>,
    handles_seen: Mutex<Vec<NativeWindowHandle>>,
}

impl FakeProbe {
    fn new(monitor: MonitorDescriptor) -> Self {
        Self {
            cursor: RwLock::new(Ok(None)),
            monitors: RwLock::new(Ok(vec![monitor])),
            fullscreen: RwLock::new(false),
            handles_seen: Mutex::new(Vec::new()),
        }
    }

    fn set_cursor(&self, cursor: Option<Point>) {
        *self.cursor.write().unwrap() = Ok(cursor);
    }

    fn set_monitors(&self, monitors: Vec<MonitorDescriptor>) {
        *self.monitors.write().unwrap() = Ok(monitors);
    }

    fn fail_monitors_once(&self) {
        *self.monitors.write().unwrap() = Err(DesktopError::MonitorEnumerationFailed);
    }
}

impl DesktopProbe for FakeProbe {
    fn cursor_position(&self) -> Result<Option<Point>, DesktopError> {
        *self.cursor.read().unwrap()
    }

    fn monitors(&self) -> Result<Vec<MonitorDescriptor>, DesktopError> {
        self.monitors.read().unwrap().clone()
    }

    fn foreground_is_fullscreen(
        &self,
        _selected_monitor: &MonitorDescriptor,
        dashy_window_handles: &[NativeWindowHandle],
    ) -> bool {
        *self.handles_seen.lock().unwrap() = dashy_window_handles.to_vec();
        *self.fullscreen.read().unwrap()
    }
}

struct FakeSettings(RwLock<Result<AppSettings, String>>);

impl FakeSettings {
    fn new(settings: AppSettings) -> Self {
        Self(RwLock::new(Ok(settings)))
    }

    fn set(&self, settings: AppSettings) {
        *self.0.write().unwrap() = Ok(settings);
    }
}

impl SettingsSource for FakeSettings {
    fn current(&self) -> Result<AppSettings, String> {
        self.0.read().unwrap().clone()
    }
}

fn monitor(id: &str, name: &str, x: i32, width: u32, primary: bool) -> MonitorDescriptor {
    monitor_at_scale(
        id,
        name,
        x,
        width,
        primary,
        dashy::desktop::edge::MonitorScale::ONE,
    )
}

fn monitor_at_scale(
    id: &str,
    name: &str,
    x: i32,
    width: u32,
    primary: bool,
    scale: dashy::desktop::edge::MonitorScale,
) -> MonitorDescriptor {
    MonitorDescriptor {
        id: id.into(),
        name: name.into(),
        monitor_rect: Rect {
            x,
            y: 0,
            width,
            height: 1080,
        },
        work_rect: MonitorWorkArea::new(x, 0, width, 1040).unwrap(),
        scale,
        primary,
    }
}

struct Fixture {
    controller: DesktopController,
    window: Arc<FakeWindow>,
    probe: Arc<FakeProbe>,
    settings: Arc<FakeSettings>,
}

fn configured_settings() -> AppSettings {
    AppSettings {
        onboarding_completed: true,
        enabled_providers: ProviderId::ALL.to_vec(),
        ..AppSettings::default()
    }
}

impl Fixture {
    fn new() -> Self {
        let window = Arc::new(FakeWindow::default());
        let probe = Arc::new(FakeProbe::new(monitor("display-a", "Desk", 0, 1920, true)));
        let settings = Arc::new(FakeSettings::new(configured_settings()));
        let controller = DesktopController::new(probe.clone(), window.clone(), settings.clone());
        Self {
            controller,
            window,
            probe,
            settings,
        }
    }

    fn show(&self, now: Duration) {
        self.controller.show_explicit();
        assert!(self.controller.step(now).is_empty());
        self.window.clear();
    }
}

fn last_layout(actions: &[WindowAction]) -> WindowLayout {
    actions
        .iter()
        .rev()
        .find_map(|action| match action {
            WindowAction::Apply(layout) => Some(*layout),
            _ => None,
        })
        .expect("expected a window layout")
}

fn exit_token(value: &str) -> ExitToken {
    ExitToken::try_from(value).unwrap()
}

#[test]
fn current_edge_view_is_an_authoritative_typed_handshake() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.controller.current_edge_view().unwrap(),
        EdgeViewState {
            visibility: EdgeUiState::Hidden,
            placement: EdgePlacement::Right,
            provider: None,
        }
    );

    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::TogglePin(ProviderId::Claude));
    fixture.controller.step(Duration::from_millis(1));
    fixture.settings.set(AppSettings {
        placement: EdgePlacement::Left,
        ..configured_settings()
    });

    assert_eq!(
        fixture.controller.current_edge_view().unwrap(),
        EdgeViewState {
            visibility: EdgeUiState::Pinned,
            placement: EdgePlacement::Left,
            provider: Some(ProviderId::Claude),
        }
    );
}

#[test]
fn incomplete_onboarding_keeps_the_native_surface_hidden() {
    let fixture = Fixture::new();
    fixture.settings.set(AppSettings::default());
    fixture.controller.show_explicit();
    assert!(fixture.controller.step(Duration::from_millis(1)).is_empty());
    assert_eq!(fixture.controller.state(), EdgeUiState::Hidden);
    assert!(fixture
        .window
        .actions()
        .iter()
        .all(|action| { !matches!(action, WindowAction::Apply(layout) if layout.visible) }));
}

#[test]
fn enabled_provider_changes_resize_the_collapsed_native_surface() {
    let fixture = Fixture::new();
    fixture.settings.set(AppSettings {
        enabled_providers: vec![ProviderId::Claude, ProviderId::GitHub],
        ..configured_settings()
    });
    fixture.controller.show_explicit();
    assert!(fixture.controller.step(Duration::ZERO).is_empty());
    assert_eq!(last_layout(&fixture.window.actions()).size.height, 260);

    fixture.window.clear();
    fixture.settings.set(AppSettings {
        enabled_providers: vec![ProviderId::Claude],
        ..configured_settings()
    });
    assert!(fixture.controller.step(Duration::from_millis(1)).is_empty());
    assert_eq!(last_layout(&fixture.window.actions()).size.height, 180);
}

#[test]
fn monitor_enumeration_failure_cannot_keep_a_disabled_surface_visible() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture.settings.set(AppSettings {
        onboarding_completed: false,
        ..configured_settings()
    });
    fixture.probe.fail_monitors_once();
    fixture.controller.show_explicit();
    fixture
        .controller
        .queue_interaction(EdgeInteraction::SelectProvider(ProviderId::Codex));

    assert_eq!(
        fixture.controller.step(Duration::from_millis(1)),
        vec![DesktopError::MonitorEnumerationFailed]
    );
    assert_eq!(fixture.controller.state(), EdgeUiState::Hidden);

    fixture.controller.show_explicit();
    fixture
        .controller
        .queue_interaction(EdgeInteraction::TogglePin(ProviderId::Codex));
    assert_eq!(
        fixture
            .controller
            .step(Duration::from_millis(1) + EXIT_FALLBACK),
        vec![DesktopError::MonitorEnumerationFailed]
    );

    let actions = fixture.window.actions();
    assert_eq!(fixture.controller.state(), EdgeUiState::Hidden);
    assert!(actions.iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if !layout.visible
    )));
    assert!(!actions.iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if layout.visible
    )));
    assert!(!actions.contains(&WindowAction::Focus));
}

#[test]
fn missing_monitor_cannot_keep_a_disabled_pinned_surface_visible() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::TogglePin(ProviderId::Claude));
    assert!(fixture.controller.step(Duration::from_millis(1)).is_empty());
    assert_eq!(fixture.controller.state(), EdgeUiState::Pinned);
    fixture.window.clear();

    fixture.settings.set(AppSettings {
        enabled_providers: Vec::new(),
        ..configured_settings()
    });
    fixture.probe.set_monitors(Vec::new());
    fixture.controller.show_explicit();
    fixture
        .controller
        .queue_interaction(EdgeInteraction::SelectProvider(ProviderId::GitHub));

    assert_eq!(
        fixture.controller.step(Duration::from_millis(2)),
        vec![DesktopError::NoMonitorAvailable]
    );
    assert_eq!(fixture.controller.state(), EdgeUiState::Hidden);

    fixture.controller.show_explicit();
    fixture
        .controller
        .queue_interaction(EdgeInteraction::TogglePin(ProviderId::GitHub));
    assert_eq!(
        fixture
            .controller
            .step(Duration::from_millis(2) + EXIT_FALLBACK),
        vec![DesktopError::NoMonitorAvailable]
    );

    let actions = fixture.window.actions();
    assert_eq!(fixture.controller.state(), EdgeUiState::Hidden);
    assert!(actions.iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if !layout.visible
    )));
    assert!(!actions.iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if layout.visible
    )));
    assert!(!actions.contains(&WindowAction::Focus));
}

#[test]
fn proximity_reveal_never_focuses_but_explicit_show_does() {
    let fixture = Fixture::new();
    fixture.probe.set_cursor(Some(Point { x: 1919, y: 500 }));

    fixture.controller.step(Duration::ZERO);
    fixture.controller.step(Duration::from_millis(101));
    let proximity_actions = fixture.window.actions();
    assert!(proximity_actions.iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if layout.visible
    )));
    assert!(!proximity_actions.contains(&WindowAction::Focus));

    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(102));
    assert!(fixture.controller.begin_exit(exit_token("exit-reset")));
    fixture
        .controller
        .exit_animation_complete(exit_token("exit-reset"));
    fixture.controller.step(Duration::from_millis(103));
    fixture.window.clear();
    fixture.controller.show_explicit();
    fixture.controller.step(Duration::from_millis(104));

    assert!(fixture.window.actions().contains(&WindowAction::Focus));
}

#[test]
fn provider_hover_expands_inward_without_requesting_native_focus() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    let rail = dashy::desktop::edge::window_layout(
        EdgePlacement::Right,
        MonitorWorkArea::new(0, 0, 1920, 1040).unwrap(),
        EdgeUiState::RailVisible,
        None,
    );

    fixture
        .controller
        .queue_interaction(EdgeInteraction::SelectProvider(ProviderId::Codex));
    fixture.controller.step(Duration::from_millis(1));
    let actions = fixture.window.actions();
    let card = last_layout(&actions);

    assert!(!actions.contains(&WindowAction::Focus));
    assert!(card.size.width > rail.size.width);
    assert_eq!(card.position.x + card.size.width as i32, 1920);
    assert_eq!(rail.position.x + rail.size.width as i32, 1920);
}

#[test]
fn proximity_reveal_then_provider_hover_never_requests_native_focus() {
    let fixture = Fixture::new();
    fixture.probe.set_cursor(Some(Point { x: 1919, y: 500 }));

    fixture.controller.step(Duration::ZERO);
    fixture.controller.step(Duration::from_millis(101));
    fixture.window.clear();

    fixture
        .controller
        .queue_interaction(EdgeInteraction::SelectProvider(ProviderId::Codex));
    fixture.controller.step(Duration::from_millis(102));

    let actions = fixture.window.actions();
    assert!(!actions.contains(&WindowAction::Focus));
    assert!(actions.iter().any(|action| matches!(
        action,
        WindowAction::Emit(view)
            if view.visibility == EdgeUiState::CardVisible
                && view.provider == Some(ProviderId::Codex)
    )));
}

#[test]
fn pin_activation_requests_native_focus() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture.window.clear();

    fixture
        .controller
        .queue_interaction(EdgeInteraction::TogglePin(ProviderId::Codex));
    fixture.controller.step(Duration::from_millis(1));

    assert!(fixture.window.actions().contains(&WindowAction::Focus));
}

#[test]
fn losing_focus_while_pinned_semantically_unpins() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::TogglePin(ProviderId::Claude));
    fixture.controller.step(Duration::from_millis(1));
    assert_eq!(fixture.controller.state(), EdgeUiState::Pinned);
    fixture.window.clear();

    fixture.controller.focus_lost();
    fixture.controller.step(Duration::from_millis(2));

    assert_eq!(fixture.controller.state(), EdgeUiState::CardVisible);
    assert!(fixture.window.actions().iter().any(|action| matches!(
        action,
        WindowAction::Emit(view) if view.visibility == EdgeUiState::CardVisible
    )));
}

#[test]
fn semantic_notch_sequence_emits_one_coherent_view_for_each_state_change() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);

    let steps = [
        (
            EdgeInteraction::SelectProvider(ProviderId::Claude),
            EdgeUiState::CardVisible,
            Some(ProviderId::Claude),
        ),
        (
            EdgeInteraction::TogglePin(ProviderId::Claude),
            EdgeUiState::Pinned,
            Some(ProviderId::Claude),
        ),
        (
            EdgeInteraction::OutsideClick,
            EdgeUiState::CardVisible,
            Some(ProviderId::Claude),
        ),
        (EdgeInteraction::Escape, EdgeUiState::RailVisible, None),
        (EdgeInteraction::Escape, EdgeUiState::Hidden, None),
    ];

    for (index, (interaction, visibility, provider)) in steps.into_iter().enumerate() {
        fixture.controller.queue_interaction(interaction);
        assert!(fixture
            .controller
            .step(Duration::from_millis(index as u64 + 1))
            .is_empty());
        let emitted = fixture
            .window
            .actions()
            .into_iter()
            .filter_map(|action| match action {
                WindowAction::Emit(view) => Some(view),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            emitted,
            vec![EdgeViewState {
                visibility,
                placement: EdgePlacement::Right,
                provider,
            }]
        );
        fixture.window.clear();
    }
}

#[test]
fn hidden_layout_waits_for_css_acknowledgement() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(1));

    let before_ack = fixture.window.actions();
    assert!(before_ack.iter().any(|action| matches!(
        action,
        WindowAction::Emit(view) if view.visibility == EdgeUiState::Hidden
    )));
    assert!(!before_ack.iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if !layout.visible
    )));

    assert!(fixture.controller.begin_exit(exit_token("exit-ack")));
    fixture
        .controller
        .exit_animation_complete(exit_token("exit-ack"));
    fixture.controller.step(Duration::from_millis(2));
    assert!(!last_layout(&fixture.window.actions()).visible);
}

#[test]
fn hidden_layout_has_a_bounded_fallback_when_css_does_not_acknowledge() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(1));
    assert!(fixture.controller.begin_exit(exit_token("exit-fallback")));
    fixture
        .controller
        .step(Duration::from_millis(1) + EXIT_FALLBACK - Duration::from_millis(1));
    assert!(!fixture.window.actions().iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if !layout.visible
    )));

    fixture
        .controller
        .step(Duration::from_millis(1) + EXIT_FALLBACK);
    assert!(!last_layout(&fixture.window.actions()).visible);
    assert!(!fixture.controller.begin_exit(exit_token("exit-too-late")));
    assert!(!fixture
        .controller
        .exit_animation_complete(exit_token("exit-fallback")));
}

#[test]
fn interrupted_exit_a_can_never_acknowledge_exit_b() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(1));
    assert!(fixture.controller.begin_exit(exit_token("exit-a")));

    fixture.controller.show_explicit();
    fixture.controller.step(Duration::from_millis(2));
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(3));
    assert!(fixture.controller.begin_exit(exit_token("exit-b")));
    fixture.window.clear();

    assert!(!fixture
        .controller
        .exit_animation_complete(exit_token("exit-a")));
    fixture.controller.step(Duration::from_millis(4));
    assert!(!fixture.window.actions().iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if !layout.visible
    )));

    assert!(fixture
        .controller
        .exit_animation_complete(exit_token("exit-b")));
    fixture.controller.step(Duration::from_millis(5));
    assert!(!last_layout(&fixture.window.actions()).visible);
}

#[test]
fn acknowledged_exit_a_cannot_hide_a_same_step_replacement_b() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(1));
    assert!(fixture.controller.begin_exit(exit_token("exit-a")));
    assert!(fixture
        .controller
        .exit_animation_complete(exit_token("exit-a")));

    fixture.settings.set(AppSettings {
        placement: EdgePlacement::Left,
        ..configured_settings()
    });
    fixture.window.clear();
    fixture.controller.step(Duration::from_millis(2));

    assert!(!fixture.window.actions().iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if !layout.visible
    )));
    assert!(fixture.controller.begin_exit(exit_token("exit-b")));
    assert!(!fixture
        .controller
        .exit_animation_complete(exit_token("exit-a")));
    assert!(fixture
        .controller
        .exit_animation_complete(exit_token("exit-b")));
    fixture.controller.step(Duration::from_millis(3));

    let hidden = last_layout(&fixture.window.actions());
    assert!(!hidden.visible);
    assert_eq!(hidden.placement, EdgePlacement::Left);
}

#[test]
fn missing_or_wrong_exit_token_never_acknowledges_pending_hide() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(1));

    assert!(!fixture
        .controller
        .exit_animation_complete(exit_token("exit-without-begin")));
    fixture.controller.step(Duration::from_millis(2));
    assert!(fixture.controller.begin_exit(exit_token("exit-correct")));
    assert!(!fixture
        .controller
        .exit_animation_complete(exit_token("exit-wrong")));
    fixture.controller.step(Duration::from_millis(3));
    assert!(!fixture.window.actions().iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if !layout.visible
    )));
}

#[test]
fn begin_exit_is_rejected_after_reveal_cancels_the_pending_hide() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(1));
    fixture.controller.show_explicit();
    fixture.controller.step(Duration::from_millis(2));

    assert!(!fixture.controller.begin_exit(exit_token("exit-cancelled")));
}

#[test]
fn settings_and_monitor_geometry_changes_reposition_on_the_next_step() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture.settings.set(AppSettings {
        placement: EdgePlacement::Left,
        ..configured_settings()
    });
    fixture.controller.step(Duration::from_millis(1));
    assert_eq!(last_layout(&fixture.window.actions()).position.x, 0);

    fixture.window.clear();
    fixture
        .probe
        .set_monitors(vec![monitor("display-a", "Desk", -1600, 1600, true)]);
    fixture.controller.step(Duration::from_millis(2));
    let moved = last_layout(&fixture.window.actions());
    assert_eq!(moved.position.x, -1600);
    assert!(moved.rect().width <= 1600);
}

#[test]
fn dpi_only_monitor_change_repositions_with_scaled_logical_bounds() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture.window.clear();
    fixture.probe.set_monitors(vec![monitor_at_scale(
        "display-a",
        "Desk",
        0,
        1920,
        true,
        dashy::desktop::edge::MonitorScale::from_effective_dpi(144, 144).unwrap(),
    )]);

    fixture.controller.step(Duration::from_millis(1));

    let scaled = last_layout(&fixture.window.actions());
    assert_eq!((scaled.size.width, scaled.size.height), (105, 510));
    assert_eq!(
        scaled.position.x + i32::try_from(scaled.size.width).unwrap(),
        1920
    );
}

#[test]
fn monitor_topology_refresh_retries_until_success_and_then_acknowledges() {
    let fixture = Fixture::new();
    fixture.controller.step(Duration::ZERO);
    let attempts = AtomicUsize::new(0);
    let refresh = || {
        if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err("simulated native menu failure".to_string())
        } else {
            Ok(())
        }
    };

    assert!(fixture
        .controller
        .refresh_monitor_topology_if_needed(&refresh)
        .is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture
            .controller
            .refresh_monitor_topology_if_needed(&refresh),
        Ok(true)
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(
        fixture
            .controller
            .refresh_monitor_topology_if_needed(&refresh),
        Ok(false)
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    fixture.probe.set_monitors(vec![
        monitor("display-a", "Desk", 0, 1920, true),
        monitor("display-b", "Projector", 1920, 1280, false),
    ]);
    fixture.controller.step(Duration::from_millis(40));

    assert_eq!(
        fixture
            .controller
            .refresh_monitor_topology_if_needed(&refresh),
        Ok(true)
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[test]
fn fullscreen_suppression_hides_and_override_keeps_the_notch_eligible() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    *fixture.probe.fullscreen.write().unwrap() = true;
    fixture.controller.step(Duration::from_millis(1));
    let suppressed = last_layout(&fixture.window.actions());
    assert!(!suppressed.visible);
    assert!(!suppressed.always_on_top);
    assert_eq!(
        fixture.probe.handles_seen.lock().unwrap().as_slice(),
        &[41, 42]
    );

    fixture.settings.set(AppSettings {
        always_show_over_fullscreen: true,
        ..configured_settings()
    });
    fixture.controller.show_explicit();
    fixture.controller.step(Duration::from_millis(2));
    let visible = last_layout(&fixture.window.actions());
    assert!(visible.visible);
    assert!(visible.always_on_top);
}

#[test]
fn recoverable_probe_and_window_errors_do_not_terminate_later_steps() {
    let fixture = Fixture::new();
    fixture.probe.fail_monitors_once();
    let first_errors = fixture.controller.step(Duration::ZERO);
    assert_eq!(first_errors, vec![DesktopError::MonitorEnumerationFailed]);

    fixture
        .probe
        .set_monitors(vec![monitor("display-a", "Desk", 0, 1920, true)]);
    fixture.window.fail_next();
    fixture.controller.show_explicit();
    assert_eq!(
        fixture.controller.step(Duration::from_millis(1)),
        vec![DesktopError::WindowOperationFailed]
    );

    fixture.controller.show_explicit();
    assert!(fixture.controller.step(Duration::from_millis(2)).is_empty());
    assert!(fixture.window.actions().contains(&WindowAction::Focus));
}

#[test]
fn newer_suppressed_layout_discards_a_failed_visible_retry() {
    let fixture = Fixture::new();
    fixture.window.fail_next_apply();
    fixture.controller.show_explicit();
    assert_eq!(
        fixture.controller.step(Duration::ZERO),
        vec![DesktopError::WindowOperationFailed]
    );
    fixture.window.clear();

    *fixture.probe.fullscreen.write().unwrap() = true;
    assert!(fixture.controller.step(Duration::from_millis(1)).is_empty());
    let suppression_actions = fixture.window.actions();
    assert_eq!(
        suppression_actions
            .iter()
            .filter(|action| matches!(action, WindowAction::Apply(_)))
            .count(),
        1
    );
    assert!(matches!(
        suppression_actions.as_slice(),
        [WindowAction::Apply(layout), ..] if !layout.visible
    ));

    fixture.window.clear();
    assert!(fixture.controller.step(Duration::from_millis(2)).is_empty());
    assert!(!fixture.window.actions().iter().any(|action| matches!(
        action,
        WindowAction::Apply(layout) if layout.visible
    )));
}

#[test]
fn newer_hidden_view_discards_a_failed_visible_view_retry() {
    let fixture = Fixture::new();
    fixture.window.fail_next_emit();
    fixture.controller.show_explicit();
    assert_eq!(
        fixture.controller.step(Duration::ZERO),
        vec![DesktopError::WindowOperationFailed]
    );
    fixture.window.clear();

    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    assert!(fixture.controller.step(Duration::from_millis(1)).is_empty());
    assert_eq!(
        fixture.window.actions(),
        vec![WindowAction::Emit(EdgeViewState {
            visibility: EdgeUiState::Hidden,
            placement: EdgePlacement::Right,
            provider: None,
        })]
    );

    fixture.window.clear();
    assert!(fixture.controller.step(Duration::from_millis(2)).is_empty());
    assert!(!fixture.window.actions().iter().any(|action| matches!(
        action,
        WindowAction::Emit(view) if view.visibility == EdgeUiState::RailVisible
    )));
}

#[test]
fn newer_hidden_layout_replaces_the_css_deferred_hide_destination() {
    let fixture = Fixture::new();
    fixture.show(Duration::ZERO);
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Escape);
    fixture.controller.step(Duration::from_millis(1));

    fixture.settings.set(AppSettings {
        placement: EdgePlacement::Left,
        ..configured_settings()
    });
    fixture.controller.step(Duration::from_millis(2));
    fixture.window.clear();

    assert!(fixture.controller.begin_exit(exit_token("exit-moved")));
    fixture
        .controller
        .exit_animation_complete(exit_token("exit-moved"));
    fixture.controller.step(Duration::from_millis(3));
    let hidden = last_layout(&fixture.window.actions());
    assert_eq!(hidden.placement, EdgePlacement::Left);
    assert!(!hidden.visible);
    assert!(hidden.position.x < 0);
}

#[test]
fn activation_does_not_focus_when_final_state_is_hidden_or_suppressed() {
    let fixture = Fixture::new();
    *fixture.probe.fullscreen.write().unwrap() = true;
    fixture.controller.show_explicit();
    fixture.controller.step(Duration::ZERO);
    assert_eq!(fixture.controller.state(), EdgeUiState::Suppressed);
    assert!(!fixture.window.actions().contains(&WindowAction::Focus));

    *fixture.probe.fullscreen.write().unwrap() = false;
    fixture.window.clear();
    fixture.controller.show_explicit();
    fixture
        .controller
        .queue_interaction(EdgeInteraction::Dismiss);
    fixture.controller.step(Duration::from_millis(1));
    assert_eq!(fixture.controller.state(), EdgeUiState::Hidden);
    assert!(!fixture.window.actions().contains(&WindowAction::Focus));
}

#[test]
fn failed_show_apply_blocks_focus_but_visible_no_layout_activation_focuses() {
    let fixture = Fixture::new();
    fixture.window.fail_next_apply();
    fixture.controller.show_explicit();
    assert_eq!(
        fixture.controller.step(Duration::ZERO),
        vec![DesktopError::WindowOperationFailed]
    );
    assert!(!fixture.window.actions().contains(&WindowAction::Focus));

    fixture.window.clear();
    fixture.controller.show_explicit();
    assert!(fixture.controller.step(Duration::from_millis(1)).is_empty());
    assert!(fixture.window.actions().contains(&WindowAction::Focus));

    fixture.window.clear();
    fixture.controller.show_explicit();
    assert!(fixture.controller.step(Duration::from_millis(2)).is_empty());
    assert_eq!(fixture.window.actions(), vec![WindowAction::Focus]);
}

#[test]
fn monitor_resolution_uses_exact_id_then_safe_recovery_then_primary() {
    let primary = monitor("display-a", "Built-in", 0, 1920, true);
    let desk = monitor("display-new", "Desk", 1920, 2560, false);
    let saved = MonitorPreference {
        id: "display-old".into(),
        name: "Desk".into(),
        last_work_area: StoredMonitorRect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1040,
        },
    };

    let recovered = resolve_monitor(Some(&saved), &[primary.clone(), desk.clone()]).unwrap();
    assert_eq!(recovered.monitor.id, "display-new");
    assert_eq!(
        recovered.resolution,
        MonitorResolution::RecoveredNameAndGeometry
    );

    let unavailable = MonitorPreference {
        name: "Gone".into(),
        ..saved.clone()
    };
    let fallback = resolve_monitor(Some(&unavailable), &[primary.clone(), desk]).unwrap();
    assert_eq!(fallback.monitor.id, primary.id);
    assert_eq!(fallback.resolution, MonitorResolution::PrimaryFallback);
    assert_eq!(
        unavailable.id, "display-old",
        "resolution must not mutate the saved preference"
    );
}

#[test]
fn menu_model_has_stable_ids_checked_choices_and_preserves_unavailable_monitor() {
    let monitors = vec![
        monitor("display-a", "Built-in", 0, 1920, true),
        monitor("display-b", "Desk", 1920, 2560, false),
    ];
    let settings = AppSettings {
        placement: EdgePlacement::Top,
        monitor: Some(MonitorPreference {
            id: "display-gone".into(),
            name: "Projector".into(),
            last_work_area: StoredMonitorRect {
                x: -1280,
                y: 0,
                width: 1280,
                height: 720,
            },
        }),
        ..configured_settings()
    };

    let labels = TrayLabels {
        right: "Rechts".into(),
        left: "Links".into(),
        top: "Oben".into(),
        unavailable: "Nicht verfügbar".into(),
        ..TrayLabels::default()
    };
    let spec = build_menu_spec(&labels, &settings, &monitors).unwrap();
    let ids = spec
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    for required in [
        "show",
        "refresh_all",
        "placement_right",
        "placement_left",
        "placement_top",
        "monitor_primary",
        "monitor_display-a",
        "monitor_display-b",
        "monitor_display-gone",
        "settings",
        "quit",
    ] {
        assert!(ids.contains(&required), "missing menu id {required}");
    }
    assert!(spec.item("placement_top").unwrap().checked);
    assert_eq!(spec.item("placement_right").unwrap().label, "Rechts");
    assert_eq!(spec.item("placement_left").unwrap().label, "Links");
    assert_eq!(spec.item("placement_top").unwrap().label, "Oben");
    assert!(spec.item("monitor_primary").unwrap().checked);
    let unavailable = spec.item("monitor_display-gone").unwrap();
    assert!(!unavailable.enabled);
    assert!(!unavailable.checked);
    assert_eq!(unavailable.label, "Projector (Nicht verfügbar)");
}

#[test]
fn tray_labels_accept_unicode_but_reject_empty_control_and_overlong_inputs() {
    let valid = TrayLabels {
        show: "הצג את Dashy".into(),
        ..TrayLabels::default()
    };
    assert!(valid.validate().is_ok());

    for invalid_show in [String::new(), "bad\nlabel".into(), "界".repeat(81)] {
        let invalid = TrayLabels {
            show: invalid_show,
            ..TrayLabels::default()
        };
        assert!(invalid.validate().is_err());
    }
}
