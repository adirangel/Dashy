use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::{Deserialize, Deserializer, Serialize};
use tauri::{Emitter, Manager, Runtime, WebviewWindow};
#[cfg(not(windows))]
use tauri::{PhysicalPosition, PhysicalSize};
use tokio::sync::watch;

use super::{
    edge::{
        window_layout_scaled, EdgeEffect, EdgeInput, EdgeInteraction, EdgeMachine, EdgeUiState,
        EdgeViewState, WindowLayout,
    },
    menu::resolve_monitor,
    platform::{DesktopError, DesktopProbe, NativeWindowHandle},
    settings::{AppSettings, SettingsService},
};

pub const TICK_INTERVAL: Duration = Duration::from_millis(40);
pub const EXIT_FALLBACK: Duration = Duration::from_millis(260);
pub const MAX_EXIT_TOKEN_LENGTH: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExitToken(String);

impl TryFrom<&str> for ExitToken {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > MAX_EXIT_TOKEN_LENGTH
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err("invalid exit token".to_string());
        }
        Ok(Self(value.to_string()))
    }
}

impl<'de> Deserialize<'de> for ExitToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_from(value.as_str()).map_err(serde::de::Error::custom)
    }
}

pub trait WindowPort: Send + Sync {
    fn apply(&self, layout: &WindowLayout) -> Result<(), DesktopError>;
    fn emit_view(&self, state: &EdgeViewState) -> Result<(), DesktopError>;
    fn focus(&self) -> Result<(), DesktopError>;
    fn native_handles(&self) -> Vec<NativeWindowHandle>;
}

pub trait SettingsSource: Send + Sync {
    fn current(&self) -> Result<AppSettings, String>;
}

impl SettingsSource for SettingsService {
    fn current(&self) -> Result<AppSettings, String> {
        SettingsService::current(self)
    }
}

#[derive(Clone, Debug)]
enum ControllerEvent {
    Interaction(EdgeInteraction),
    ExplicitShow,
    FocusLost,
    ExitAnimationComplete(ExitToken),
}

struct PendingHide {
    started: Duration,
    layout: WindowLayout,
    token: Option<ExitToken>,
}

#[derive(Default)]
struct OperationPlan {
    layout: Option<WindowLayout>,
    view: Option<EdgeViewState>,
    focus_requested: bool,
}

#[derive(Default)]
struct ControllerCore {
    machine: EdgeMachine,
    events: VecDeque<ControllerEvent>,
    last_surface_input: Option<EdgeInput>,
    pending_hide: Option<PendingHide>,
    acknowledged_exit: Option<ExitToken>,
    initialized: bool,
    retry_layout: Option<WindowLayout>,
    retry_view: Option<EdgeViewState>,
    last_monitors: Option<Vec<super::platform::MonitorDescriptor>>,
    monitor_topology_generation: u64,
    acknowledged_monitor_topology_generation: u64,
}

pub struct DesktopController {
    probe: Arc<dyn DesktopProbe>,
    window: Arc<dyn WindowPort>,
    settings: Arc<dyn SettingsSource>,
    core: Mutex<ControllerCore>,
}

pub struct ControllerRuntime {
    cancellation: watch::Sender<bool>,
}

impl ControllerRuntime {
    pub fn cancel(&self) {
        self.cancellation.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancellation.borrow()
    }
}

pub struct TauriWindowPort<R: Runtime> {
    main: WebviewWindow<R>,
    settings: Option<WebviewWindow<R>>,
    onboarding: Option<WebviewWindow<R>>,
}

impl<R: Runtime> TauriWindowPort<R> {
    pub fn from_manager(manager: &impl Manager<R>) -> Result<Self, DesktopError> {
        let main = manager
            .get_webview_window("main")
            .ok_or(DesktopError::WindowOperationFailed)?;
        Ok(Self {
            main,
            settings: manager.get_webview_window("settings"),
            onboarding: manager.get_webview_window("onboarding"),
        })
    }
}

impl<R: Runtime> WindowPort for TauriWindowPort<R> {
    fn apply(&self, layout: &WindowLayout) -> Result<(), DesktopError> {
        #[cfg(windows)]
        {
            let handle = self
                .main
                .hwnd()
                .map_err(|_| DesktopError::WindowOperationFailed)?;
            super::windows::apply_window_bounds(handle.0 as NativeWindowHandle, layout)?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            self.main
                .set_size(PhysicalSize::new(layout.size.width, layout.size.height))
                .map_err(|_| DesktopError::WindowOperationFailed)?;
            self.main
                .set_position(PhysicalPosition::new(layout.position.x, layout.position.y))
                .map_err(|_| DesktopError::WindowOperationFailed)?;
            self.main
                .set_always_on_top(layout.always_on_top)
                .map_err(|_| DesktopError::WindowOperationFailed)?;
            if layout.visible {
                self.main
                    .show()
                    .map_err(|_| DesktopError::WindowOperationFailed)
            } else {
                self.main
                    .hide()
                    .map_err(|_| DesktopError::WindowOperationFailed)
            }
        }
    }

    fn emit_view(&self, state: &EdgeViewState) -> Result<(), DesktopError> {
        self.main
            .emit("dashy://edge-view", state)
            .map_err(|_| DesktopError::EventEmissionFailed)
    }

    fn focus(&self) -> Result<(), DesktopError> {
        self.main
            .set_focus()
            .map_err(|_| DesktopError::WindowOperationFailed)
    }

    fn native_handles(&self) -> Vec<NativeWindowHandle> {
        #[cfg(windows)]
        {
            let mut handles = Vec::with_capacity(3);
            if let Ok(handle) = self.main.hwnd() {
                handles.push(handle.0 as NativeWindowHandle);
            }
            if let Some(settings) = &self.settings {
                if let Ok(handle) = settings.hwnd() {
                    handles.push(handle.0 as NativeWindowHandle);
                }
            }
            if let Some(onboarding) = &self.onboarding {
                if let Ok(handle) = onboarding.hwnd() {
                    handles.push(handle.0 as NativeWindowHandle);
                }
            }
            handles
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
}

pub fn start_controller_runtime(
    controller: Arc<DesktopController>,
    mut settings_changes: watch::Receiver<AppSettings>,
    refresh_monitor_topology: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
) -> ControllerRuntime {
    let (cancellation, mut cancelled) = watch::channel(false);
    tauri::async_runtime::spawn(async move {
        let started = tokio::time::Instant::now();
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let _ = controller.step(started.elapsed());
                    if controller.refresh_monitor_topology_if_needed(refresh_monitor_topology.as_ref()).is_err() {
                        // The generation stays pending and is retried on the next runtime wakeup.
                    }
                }
                changed = settings_changes.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let _ = controller.step(started.elapsed());
                    if controller.refresh_monitor_topology_if_needed(refresh_monitor_topology.as_ref()).is_err() {
                        // The generation stays pending and is retried on the next runtime wakeup.
                    }
                }
                changed = cancelled.changed() => {
                    if changed.is_err() || *cancelled.borrow() {
                        break;
                    }
                }
            }
        }
    });
    ControllerRuntime { cancellation }
}

impl DesktopController {
    pub fn new(
        probe: Arc<dyn DesktopProbe>,
        window: Arc<dyn WindowPort>,
        settings: Arc<dyn SettingsSource>,
    ) -> Self {
        Self {
            probe,
            window,
            settings,
            core: Mutex::new(ControllerCore::default()),
        }
    }

    pub fn queue_interaction(&self, interaction: EdgeInteraction) {
        self.push_event(ControllerEvent::Interaction(interaction));
    }

    pub fn show_explicit(&self) {
        self.push_event(ControllerEvent::ExplicitShow);
    }

    pub fn focus_lost(&self) {
        self.push_event(ControllerEvent::FocusLost);
    }

    pub fn begin_exit(&self, token: ExitToken) -> bool {
        let mut core = self.core.lock().expect("desktop controller lock poisoned");
        if core.machine.state() != EdgeUiState::Hidden {
            return false;
        }
        let Some(pending) = core.pending_hide.as_mut() else {
            return false;
        };
        match &pending.token {
            Some(current) => current == &token,
            None => {
                pending.token = Some(token);
                true
            }
        }
    }

    pub fn exit_animation_complete(&self, token: ExitToken) -> bool {
        let accepted = {
            let core = self.core.lock().expect("desktop controller lock poisoned");
            core.machine.state() == EdgeUiState::Hidden
                && core
                    .pending_hide
                    .as_ref()
                    .and_then(|pending| pending.token.as_ref())
                    == Some(&token)
        };
        if accepted {
            self.push_event(ControllerEvent::ExitAnimationComplete(token));
        }
        accepted
    }

    pub fn state(&self) -> EdgeUiState {
        self.core
            .lock()
            .expect("desktop controller lock poisoned")
            .machine
            .state()
    }

    pub fn current_edge_view(&self) -> Result<EdgeViewState, String> {
        let placement = self.settings.current()?.placement;
        Ok(self
            .core
            .lock()
            .map_err(|_| "desktop controller lock poisoned".to_string())?
            .machine
            .current_view(placement))
    }

    pub fn refresh_monitor_topology_if_needed(
        &self,
        refresh: &(dyn Fn() -> Result<(), String> + Send + Sync),
    ) -> Result<bool, String> {
        let generation = {
            let core = self.core.lock().expect("desktop controller lock poisoned");
            if core.monitor_topology_generation == core.acknowledged_monitor_topology_generation {
                return Ok(false);
            }
            core.monitor_topology_generation
        };

        refresh()?;

        let mut core = self.core.lock().expect("desktop controller lock poisoned");
        if core.acknowledged_monitor_topology_generation != generation {
            core.acknowledged_monitor_topology_generation = generation;
        }
        Ok(true)
    }

    pub fn step(&self, now: Duration) -> Vec<DesktopError> {
        let settings = match self.settings.current() {
            Ok(settings) => settings,
            Err(_) => return vec![DesktopError::SettingsUnavailable],
        };
        let surface_enabled =
            settings.onboarding_completed && !settings.enabled_providers.is_empty();
        let monitors = match self.probe.monitors() {
            Ok(monitors) => monitors,
            Err(error) if !surface_enabled => {
                return self.step_without_monitor(now, &settings, error);
            }
            Err(error) => return vec![error],
        };
        {
            let mut core = self.core.lock().expect("desktop controller lock poisoned");
            if core.last_monitors.as_ref() != Some(&monitors) {
                core.last_monitors = Some(monitors.clone());
                core.monitor_topology_generation = core.monitor_topology_generation.wrapping_add(1);
                if core.monitor_topology_generation == core.acknowledged_monitor_topology_generation
                {
                    core.monitor_topology_generation =
                        core.monitor_topology_generation.wrapping_add(1);
                }
            }
        }
        let selected = match resolve_monitor(settings.monitor.as_ref(), &monitors) {
            Some(selected) => selected.monitor,
            None if !surface_enabled => {
                return self.step_without_monitor(now, &settings, DesktopError::NoMonitorAvailable);
            }
            None => return vec![DesktopError::NoMonitorAvailable],
        };
        let mut errors = Vec::new();
        let cursor = if surface_enabled {
            match self.probe.cursor_position() {
                Ok(cursor) => cursor,
                Err(error) => {
                    errors.push(error);
                    None
                }
            }
        } else {
            None
        };
        let fullscreen = surface_enabled
            && self
                .probe
                .foreground_is_fullscreen(&selected, &self.window.native_handles());
        let input = EdgeInput {
            cursor,
            placement: settings.placement,
            work_area: selected.work_rect,
            scale: selected.scale,
            foreground_fullscreen: fullscreen,
            always_show_over_fullscreen: settings.always_show_over_fullscreen,
            interaction: None,
        };
        self.core
            .lock()
            .expect("desktop controller lock poisoned")
            .last_surface_input = Some(input);

        self.execute_plan(now, input, surface_enabled, errors)
    }

    fn step_without_monitor(
        &self,
        now: Duration,
        settings: &AppSettings,
        monitor_error: DesktopError,
    ) -> Vec<DesktopError> {
        let input = {
            let mut core = self.core.lock().expect("desktop controller lock poisoned");
            match core.last_surface_input {
                Some(input) => Some(EdgeInput {
                    cursor: None,
                    placement: settings.placement,
                    foreground_fullscreen: false,
                    always_show_over_fullscreen: settings.always_show_over_fullscreen,
                    interaction: None,
                    ..input
                }),
                None => {
                    core.events.clear();
                    None
                }
            }
        };
        let Some(input) = input else {
            return vec![monitor_error];
        };
        self.execute_plan(now, input, false, vec![monitor_error])
    }

    fn execute_plan(
        &self,
        now: Duration,
        input: EdgeInput,
        surface_enabled: bool,
        mut errors: Vec<DesktopError>,
    ) -> Vec<DesktopError> {
        let plan = self.plan_operations(now, input, surface_enabled);
        let layout_succeeded = if let Some(layout) = plan.layout {
            match self.window.apply(&layout) {
                Ok(()) => true,
                Err(error) => {
                    errors.push(error);
                    self.core
                        .lock()
                        .expect("desktop controller lock poisoned")
                        .retry_layout = Some(layout);
                    false
                }
            }
        } else {
            true
        };
        if let Some(view) = plan.view {
            if let Err(error) = self.window.emit_view(&view) {
                errors.push(error);
                self.core
                    .lock()
                    .expect("desktop controller lock poisoned")
                    .retry_view = Some(view);
            }
        }
        if plan.focus_requested && layout_succeeded {
            if let Err(error) = self.window.focus() {
                errors.push(error);
            }
        }
        errors
    }

    fn push_event(&self, event: ControllerEvent) {
        self.core
            .lock()
            .expect("desktop controller lock poisoned")
            .events
            .push_back(event);
    }

    fn plan_operations(
        &self,
        now: Duration,
        base_input: EdgeInput,
        surface_enabled: bool,
    ) -> OperationPlan {
        let mut core = self.core.lock().expect("desktop controller lock poisoned");
        let mut plan = OperationPlan {
            layout: core
                .retry_layout
                .take()
                .filter(|layout| surface_enabled || !layout.visible),
            view: core
                .retry_view
                .take()
                .filter(|view| surface_enabled || view.visibility == EdgeUiState::Hidden),
            focus_requested: false,
        };

        let events = core.events.drain(..).collect::<Vec<_>>();
        let mut interactions = Vec::new();
        let mut focus_after_effects = false;
        for event in events {
            if !surface_enabled {
                if let ControllerEvent::ExitAnimationComplete(token) = event {
                    if core
                        .pending_hide
                        .as_ref()
                        .and_then(|pending| pending.token.as_ref())
                        == Some(&token)
                    {
                        core.acknowledged_exit = Some(token);
                    }
                }
                continue;
            }
            match event {
                ControllerEvent::Interaction(interaction) => {
                    focus_after_effects |= matches!(interaction, EdgeInteraction::TogglePin(_));
                    interactions.push(interaction);
                }
                ControllerEvent::ExplicitShow => {
                    interactions.push(EdgeInteraction::Show);
                    focus_after_effects = true;
                }
                ControllerEvent::FocusLost => {
                    if core.machine.state() == EdgeUiState::Pinned {
                        interactions.push(EdgeInteraction::OutsideClick);
                    }
                }
                ControllerEvent::ExitAnimationComplete(token) => {
                    if core
                        .pending_hide
                        .as_ref()
                        .and_then(|pending| pending.token.as_ref())
                        == Some(&token)
                    {
                        core.acknowledged_exit = Some(token);
                    }
                }
            }
        }

        if !surface_enabled && core.machine.state() != EdgeUiState::Hidden {
            interactions.push(EdgeInteraction::Dismiss);
        }

        if interactions.is_empty() {
            collect_machine_effects(&mut core, now, base_input, &mut plan);
        } else {
            for interaction in interactions {
                collect_machine_effects(
                    &mut core,
                    now,
                    EdgeInput {
                        interaction: Some(interaction),
                        ..base_input
                    },
                    &mut plan,
                );
            }
        }

        if !core.initialized {
            core.initialized = true;
            if plan.layout.is_none() {
                plan.layout = Some(window_layout_scaled(
                    base_input.placement,
                    base_input.work_area,
                    base_input.scale,
                    core.machine.state(),
                    core.machine.selected_provider(),
                ));
            }
        }

        if let Some(pending) = core.pending_hide.as_ref() {
            let fallback_elapsed = now >= pending.started && now - pending.started >= EXIT_FALLBACK;
            let acknowledged = pending.token.is_some()
                && pending.token.as_ref() == core.acknowledged_exit.as_ref();
            if (acknowledged || fallback_elapsed) && core.machine.state() == EdgeUiState::Hidden {
                plan.layout = Some(pending.layout);
                core.pending_hide = None;
                core.acknowledged_exit = None;
            }
        }
        plan.focus_requested = surface_enabled
            && focus_after_effects
            && matches!(
                core.machine.state(),
                EdgeUiState::RailVisible | EdgeUiState::CardVisible | EdgeUiState::Pinned
            );
        plan
    }
}

fn collect_machine_effects(
    core: &mut ControllerCore,
    now: Duration,
    input: EdgeInput,
    plan: &mut OperationPlan,
) {
    for effect in core.machine.advance(now, input) {
        match effect {
            EdgeEffect::ApplyWindow(layout)
                if !layout.visible && layout.view_state.visibility == EdgeUiState::Hidden =>
            {
                plan.layout = None;
                core.acknowledged_exit = None;
                core.pending_hide = Some(PendingHide {
                    started: now,
                    layout,
                    token: None,
                });
            }
            EdgeEffect::ApplyWindow(layout) => {
                if layout.visible || layout.view_state.visibility == EdgeUiState::Suppressed {
                    core.pending_hide = None;
                    core.acknowledged_exit = None;
                }
                plan.layout = Some(layout);
            }
            EdgeEffect::EmitView(view) => plan.view = Some(view),
        }
    }
}
