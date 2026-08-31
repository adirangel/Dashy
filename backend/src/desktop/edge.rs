use std::time::Duration;

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

use crate::dashboard::models::ProviderId;
use crate::desktop::settings::EdgePlacement;

pub const ACTIVATION_ZONE_PX: u32 = 28;
pub const REVEAL_DWELL: Duration = Duration::from_millis(100);
pub const CLOSE_GRACE: Duration = Duration::from_millis(420);

const SIDE_RAIL_WIDTH: u32 = 70;
const TOP_RAIL_HEIGHT: u32 = 70;
const SETTINGS_CONTROL_EXTENT: u32 = 70;
const CARD_WIDTH: u32 = 300;
const TOP_CARD_WIDTH: u32 = 340;
const CARD_HEIGHT: u32 = 360;
const BASE_DPI: u32 = 96;
const MIN_EFFECTIVE_DPI: u32 = 48;
const MAX_EFFECTIVE_DPI: u32 = 768;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorScale {
    effective_dpi: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonitorScaleError {
    NonFinite,
    OutOfRange,
    AxisMismatch,
}

impl MonitorScale {
    pub const ONE: Self = Self {
        effective_dpi: BASE_DPI,
    };

    pub fn from_effective_dpi(x: u32, y: u32) -> Result<Self, MonitorScaleError> {
        if x != y {
            return Err(MonitorScaleError::AxisMismatch);
        }
        if !(MIN_EFFECTIVE_DPI..=MAX_EFFECTIVE_DPI).contains(&x) {
            return Err(MonitorScaleError::OutOfRange);
        }
        Ok(Self { effective_dpi: x })
    }

    pub fn try_from_scale_factor(factor: f64) -> Result<Self, MonitorScaleError> {
        if !factor.is_finite() {
            return Err(MonitorScaleError::NonFinite);
        }
        let dpi = (factor * f64::from(BASE_DPI)).round();
        if dpi < f64::from(MIN_EFFECTIVE_DPI) || dpi > f64::from(MAX_EFFECTIVE_DPI) {
            return Err(MonitorScaleError::OutOfRange);
        }
        Self::from_effective_dpi(dpi as u32, dpi as u32)
    }

    pub const fn effective_dpi(self) -> u32 {
        self.effective_dpi
    }

    pub fn logical_to_physical(self, logical: u32) -> u32 {
        let scaled = u64::from(logical)
            .saturating_mul(u64::from(self.effective_dpi))
            .saturating_add(u64::from(BASE_DPI / 2))
            / u64::from(BASE_DPI);
        u32::try_from(scaled).unwrap_or(u32::MAX)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn contains(self, point: Point) -> bool {
        let right = i64::from(self.x) + i64::from(self.width);
        let bottom = i64::from(self.y) + i64::from(self.height);
        i64::from(point.x) >= i64::from(self.x)
            && i64::from(point.x) < right
            && i64::from(point.y) >= i64::from(self.y)
            && i64::from(point.y) < bottom
    }

    pub fn contains_rect(self, other: Rect) -> bool {
        let self_right = i64::from(self.x) + i64::from(self.width);
        let self_bottom = i64::from(self.y) + i64::from(self.height);
        let other_right = i64::from(other.x) + i64::from(other.width);
        let other_bottom = i64::from(other.y) + i64::from(other.height);
        i64::from(other.x) >= i64::from(self.x)
            && i64::from(other.y) >= i64::from(self.y)
            && other_right <= self_right
            && other_bottom <= self_bottom
    }

    pub fn intersects(self, other: Rect) -> bool {
        let self_right = i64::from(self.x) + i64::from(self.width);
        let self_bottom = i64::from(self.y) + i64::from(self.height);
        let other_right = i64::from(other.x) + i64::from(other.width);
        let other_bottom = i64::from(other.y) + i64::from(other.height);
        i64::from(self.x) < other_right
            && self_right > i64::from(other.x)
            && i64::from(self.y) < other_bottom
            && self_bottom > i64::from(other.y)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorWorkArea {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonitorWorkAreaError {
    ZeroExtent,
    HorizontalOverflow,
    VerticalOverflow,
}

impl MonitorWorkArea {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Result<Self, MonitorWorkAreaError> {
        if width == 0 || height == 0 {
            return Err(MonitorWorkAreaError::ZeroExtent);
        }
        if x.checked_add_unsigned(width).is_none() {
            return Err(MonitorWorkAreaError::HorizontalOverflow);
        }
        if y.checked_add_unsigned(height).is_none() {
            return Err(MonitorWorkAreaError::VerticalOverflow);
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub fn x(self) -> i32 {
        self.x
    }

    pub fn y(self) -> i32 {
        self.y
    }

    pub fn width(self) -> u32 {
        self.width
    }

    pub fn height(self) -> u32 {
        self.height
    }

    pub fn right(self) -> i32 {
        self.x
            .checked_add_unsigned(self.width)
            .expect("validated work area has a representable right edge")
    }

    pub fn bottom(self) -> i32 {
        self.y
            .checked_add_unsigned(self.height)
            .expect("validated work area has a representable bottom edge")
    }

    pub fn rect(self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgeUiState {
    Suppressed,
    Hidden,
    #[serde(rename = "rail")]
    RailVisible,
    #[serde(rename = "card")]
    CardVisible,
    Pinned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgeInteraction {
    Show,
    Dismiss,
    EnterSafeRegion,
    LeaveSafeRegion,
    SelectProvider(ProviderId),
    ClearProvider,
    TogglePin(ProviderId),
    OutsideClick,
    Escape,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EdgeInput {
    pub cursor: Option<Point>,
    pub placement: EdgePlacement,
    pub work_area: MonitorWorkArea,
    pub scale: MonitorScale,
    pub provider_count: u8,
    pub foreground_fullscreen: bool,
    pub always_show_over_fullscreen: bool,
    pub interaction: Option<EdgeInteraction>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeViewState {
    pub visibility: EdgeUiState,
    pub placement: EdgePlacement,
    pub provider: Option<ProviderId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawEdgeViewState {
    visibility: EdgeUiState,
    placement: EdgePlacement,
    #[serde(deserialize_with = "deserialize_nullable_provider")]
    provider: Option<ProviderId>,
}

fn deserialize_nullable_provider<'de, D>(deserializer: D) -> Result<Option<ProviderId>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<ProviderId>::deserialize(deserializer)
}

impl<'de> Deserialize<'de> for EdgeViewState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawEdgeViewState::deserialize(deserializer)?;
        let provider = raw.provider;
        let provider_is_coherent = match raw.visibility {
            EdgeUiState::CardVisible | EdgeUiState::Pinned => provider.is_some(),
            EdgeUiState::Suppressed | EdgeUiState::Hidden | EdgeUiState::RailVisible => {
                provider.is_none()
            }
        };
        if !provider_is_coherent {
            return Err(D::Error::custom(
                "provider must be present only for card or pinned visibility",
            ));
        }

        Ok(Self {
            visibility: raw.visibility,
            placement: raw.placement,
            provider,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowLayout {
    pub position: Point,
    pub size: Size,
    pub visible: bool,
    pub always_on_top: bool,
    pub view_state: EdgeViewState,
    pub placement: EdgePlacement,
}

impl WindowLayout {
    pub fn rect(self) -> Rect {
        Rect {
            x: self.position.x,
            y: self.position.y,
            width: self.size.width,
            height: self.size.height,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgeEffect {
    ApplyWindow(WindowLayout),
    EmitView(EdgeViewState),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LayoutContext {
    placement: EdgePlacement,
    work_area: MonitorWorkArea,
    scale: MonitorScale,
    provider_count: u8,
}

#[derive(Debug)]
pub struct EdgeMachine {
    state: EdgeUiState,
    selected_provider: Option<ProviderId>,
    safe_region_entered: bool,
    dwell_started: Option<Duration>,
    close_started: Option<Duration>,
    last_context: Option<LayoutContext>,
}

impl Default for EdgeMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl EdgeMachine {
    pub fn new() -> Self {
        Self {
            state: EdgeUiState::Hidden,
            selected_provider: None,
            safe_region_entered: false,
            dwell_started: None,
            close_started: None,
            last_context: None,
        }
    }

    pub fn state(&self) -> EdgeUiState {
        self.state
    }

    pub fn selected_provider(&self) -> Option<ProviderId> {
        self.selected_provider
    }

    pub fn current_view(&self, placement: EdgePlacement) -> EdgeViewState {
        self.view_state(placement)
    }

    pub fn advance(&mut self, now: Duration, input: EdgeInput) -> Vec<EdgeEffect> {
        let context = LayoutContext {
            placement: input.placement,
            work_area: input.work_area,
            scale: input.scale,
            provider_count: input.provider_count,
        };
        let previous_context = self.last_context.unwrap_or(context);
        let previous_view = self.view_state(previous_context.placement);
        let previous_layout = window_layout_scaled(
            previous_context.placement,
            previous_context.work_area,
            previous_context.scale,
            self.state,
            self.selected_provider,
            previous_context.provider_count,
        );

        if self.last_context.is_some_and(|last| last != context) {
            self.dwell_started = None;
            self.close_started = None;
            self.safe_region_entered = false;
        }

        if input.foreground_fullscreen && !input.always_show_over_fullscreen {
            self.suppress();
        } else {
            if self.state == EdgeUiState::Suppressed {
                self.hide();
            }
            self.handle_interaction(now, input.interaction);
            self.handle_timers(now, input);
        }

        self.last_context = Some(context);
        let next_view = self.view_state(input.placement);
        let next_layout = window_layout_scaled(
            input.placement,
            input.work_area,
            input.scale,
            self.state,
            self.selected_provider,
            input.provider_count,
        );
        let mut effects = Vec::with_capacity(2);
        if native_layout_changed(previous_layout, next_layout) {
            effects.push(EdgeEffect::ApplyWindow(next_layout));
        }
        if next_view != previous_view {
            effects.push(EdgeEffect::EmitView(next_view));
        }
        effects
    }

    fn handle_interaction(&mut self, now: Duration, interaction: Option<EdgeInteraction>) {
        match interaction {
            Some(EdgeInteraction::Show) => {
                if matches!(self.state, EdgeUiState::Hidden | EdgeUiState::Suppressed) {
                    self.state = EdgeUiState::RailVisible;
                    self.selected_provider = None;
                    self.safe_region_entered = true;
                    self.dwell_started = None;
                    self.close_started = None;
                }
            }
            Some(EdgeInteraction::Dismiss) => self.hide(),
            Some(EdgeInteraction::EnterSafeRegion) => {
                self.safe_region_entered = true;
                self.close_started = None;
            }
            Some(EdgeInteraction::LeaveSafeRegion) => {
                self.safe_region_entered = false;
                self.start_close(now);
            }
            Some(EdgeInteraction::SelectProvider(provider)) => {
                if matches!(
                    self.state,
                    EdgeUiState::RailVisible | EdgeUiState::CardVisible
                ) {
                    self.state = EdgeUiState::CardVisible;
                    self.selected_provider = Some(provider);
                    self.safe_region_entered = true;
                    self.close_started = None;
                }
            }
            Some(EdgeInteraction::ClearProvider) => {
                if self.state == EdgeUiState::CardVisible {
                    self.state = EdgeUiState::RailVisible;
                    self.selected_provider = None;
                }
            }
            Some(EdgeInteraction::TogglePin(provider)) => match self.state {
                EdgeUiState::Pinned if self.selected_provider == Some(provider) => {
                    self.state = EdgeUiState::CardVisible;
                }
                EdgeUiState::Pinned => {
                    self.selected_provider = Some(provider);
                    self.close_started = None;
                }
                EdgeUiState::RailVisible | EdgeUiState::CardVisible => {
                    self.state = EdgeUiState::Pinned;
                    self.selected_provider = Some(provider);
                    self.close_started = None;
                }
                _ => {}
            },
            Some(EdgeInteraction::OutsideClick) => {
                if self.state == EdgeUiState::Pinned {
                    self.state = EdgeUiState::CardVisible;
                    self.safe_region_entered = false;
                    self.start_close(now);
                }
            }
            Some(EdgeInteraction::Escape) => match self.state {
                EdgeUiState::Pinned | EdgeUiState::CardVisible => {
                    self.state = EdgeUiState::RailVisible;
                    self.selected_provider = None;
                    self.close_started = None;
                }
                EdgeUiState::RailVisible => self.hide(),
                EdgeUiState::Suppressed | EdgeUiState::Hidden => {}
            },
            None => {}
        }
    }

    fn handle_timers(&mut self, now: Duration, input: EdgeInput) {
        match self.state {
            EdgeUiState::Hidden => {
                let in_zone = input.cursor.is_some_and(|cursor| {
                    activation_zone_scaled(input.placement, input.work_area, input.scale)
                        .contains(cursor)
                });
                if !in_zone {
                    self.dwell_started = None;
                    return;
                }

                match self.dwell_started {
                    None => self.dwell_started = Some(now),
                    Some(started) if now < started => self.dwell_started = Some(now),
                    Some(started) if now - started >= REVEAL_DWELL => {
                        self.state = EdgeUiState::RailVisible;
                        self.dwell_started = None;
                        self.close_started = None;
                    }
                    Some(_) => {}
                }
            }
            EdgeUiState::RailVisible | EdgeUiState::CardVisible => {
                let cursor_in_activation_zone = input.cursor.is_some_and(|cursor| {
                    activation_zone_scaled(input.placement, input.work_area, input.scale)
                        .contains(cursor)
                });
                if self.safe_region_entered || cursor_in_activation_zone {
                    self.close_started = None;
                } else {
                    self.start_close(now);
                    if self
                        .close_started
                        .is_some_and(|started| now >= started && now - started >= CLOSE_GRACE)
                    {
                        self.hide();
                    }
                }
            }
            EdgeUiState::Pinned | EdgeUiState::Suppressed => {}
        }
    }

    fn start_close(&mut self, now: Duration) {
        if matches!(
            self.state,
            EdgeUiState::RailVisible | EdgeUiState::CardVisible
        ) && self.close_started.is_none()
        {
            self.close_started = Some(now);
        }
    }

    fn hide(&mut self) {
        self.state = EdgeUiState::Hidden;
        self.selected_provider = None;
        self.safe_region_entered = false;
        self.dwell_started = None;
        self.close_started = None;
    }

    fn suppress(&mut self) {
        self.hide();
        self.state = EdgeUiState::Suppressed;
    }

    fn view_state(&self, placement: EdgePlacement) -> EdgeViewState {
        EdgeViewState {
            visibility: self.state,
            placement,
            provider: self.selected_provider,
        }
    }
}

pub fn activation_zone(placement: EdgePlacement, work_area: MonitorWorkArea) -> Rect {
    activation_zone_scaled(placement, work_area, MonitorScale::ONE)
}

pub fn activation_zone_scaled(
    placement: EdgePlacement,
    work_area: MonitorWorkArea,
    scale: MonitorScale,
) -> Rect {
    let logical_thickness = scale.logical_to_physical(ACTIVATION_ZONE_PX);
    let thickness = match placement {
        EdgePlacement::Right | EdgePlacement::Left => logical_thickness.min(work_area.width),
        EdgePlacement::Top => logical_thickness.min(work_area.height),
    };
    match placement {
        EdgePlacement::Right => Rect {
            x: subtract_from_end(work_area.x, work_area.width, thickness),
            y: work_area.y,
            width: thickness,
            height: work_area.height,
        },
        EdgePlacement::Left => Rect {
            x: work_area.x,
            y: work_area.y,
            width: thickness,
            height: work_area.height,
        },
        EdgePlacement::Top => Rect {
            x: work_area.x,
            y: work_area.y,
            width: work_area.width,
            height: thickness,
        },
    }
}

pub fn visible_rect(
    placement: EdgePlacement,
    work_area: MonitorWorkArea,
    state: EdgeUiState,
) -> Rect {
    visible_rect_scaled(placement, work_area, MonitorScale::ONE, state)
}

pub fn visible_rect_scaled(
    placement: EdgePlacement,
    work_area: MonitorWorkArea,
    scale: MonitorScale,
    state: EdgeUiState,
) -> Rect {
    visible_rect_scaled_for_provider_count(placement, work_area, scale, state, 3)
}

fn visible_rect_scaled_for_provider_count(
    placement: EdgePlacement,
    work_area: MonitorWorkArea,
    scale: MonitorScale,
    state: EdgeUiState,
    provider_count: u8,
) -> Rect {
    let expanded = matches!(state, EdgeUiState::CardVisible | EdgeUiState::Pinned);
    let work_right = coordinate_end(work_area.x, work_area.width);
    let control_extent = control_logical_extent(provider_count);
    let side_rail_width = scale.logical_to_physical(SIDE_RAIL_WIDTH);
    let side_rail_height = scale.logical_to_physical(control_extent);
    let top_rail_width = scale.logical_to_physical(control_extent);
    let top_rail_height = scale.logical_to_physical(TOP_RAIL_HEIGHT);
    let side_card_width = scale.logical_to_physical(SIDE_RAIL_WIDTH + CARD_WIDTH);
    let side_card_height = scale.logical_to_physical(CARD_HEIGHT.max(control_extent));
    let top_card_width = scale.logical_to_physical(TOP_CARD_WIDTH.max(control_extent));
    let top_card_height = scale.logical_to_physical(TOP_RAIL_HEIGHT + CARD_HEIGHT);
    match (placement, expanded) {
        (EdgePlacement::Right, false) => {
            let width = side_rail_width.min(work_area.width);
            let height = side_rail_height.min(work_area.height);
            Rect {
                x: work_right.saturating_sub(u32_to_i32(width)),
                y: centered_start(work_area.y, work_area.height, height),
                width,
                height,
            }
        }
        (EdgePlacement::Left, false) => {
            let width = side_rail_width.min(work_area.width);
            let height = side_rail_height.min(work_area.height);
            Rect {
                x: work_area.x,
                y: centered_start(work_area.y, work_area.height, height),
                width,
                height,
            }
        }
        (EdgePlacement::Top, false) => {
            let width = top_rail_width.min(work_area.width);
            let height = top_rail_height.min(work_area.height);
            Rect {
                x: centered_start(work_area.x, work_area.width, width),
                y: work_area.y,
                width,
                height,
            }
        }
        (EdgePlacement::Right, true) => {
            let width = side_card_width.min(work_area.width);
            let height = side_card_height.min(work_area.height);
            Rect {
                x: work_right.saturating_sub(u32_to_i32(width)),
                y: centered_start(work_area.y, work_area.height, height),
                width,
                height,
            }
        }
        (EdgePlacement::Left, true) => {
            let width = side_card_width.min(work_area.width);
            let height = side_card_height.min(work_area.height);
            Rect {
                x: work_area.x,
                y: centered_start(work_area.y, work_area.height, height),
                width,
                height,
            }
        }
        (EdgePlacement::Top, true) => {
            let width = top_card_width.min(work_area.width);
            let height = top_card_height.min(work_area.height);
            Rect {
                x: centered_start(work_area.x, work_area.width, width),
                y: work_area.y,
                width,
                height,
            }
        }
    }
}

pub fn window_layout(
    placement: EdgePlacement,
    work_area: MonitorWorkArea,
    state: EdgeUiState,
    provider: Option<ProviderId>,
) -> WindowLayout {
    window_layout_scaled(placement, work_area, MonitorScale::ONE, state, provider, 3)
}

pub fn window_layout_scaled(
    placement: EdgePlacement,
    work_area: MonitorWorkArea,
    scale: MonitorScale,
    state: EdgeUiState,
    provider: Option<ProviderId>,
    provider_count: u8,
) -> WindowLayout {
    let visible = matches!(
        state,
        EdgeUiState::RailVisible | EdgeUiState::CardVisible | EdgeUiState::Pinned
    );
    let rect = if visible {
        visible_rect_scaled_for_provider_count(placement, work_area, scale, state, provider_count)
    } else {
        hidden_rect(placement, work_area, scale, provider_count)
    };
    let provider = if matches!(state, EdgeUiState::CardVisible | EdgeUiState::Pinned) {
        provider
    } else {
        None
    };
    let view_state = EdgeViewState {
        visibility: state,
        placement,
        provider,
    };
    WindowLayout {
        position: Point {
            x: rect.x,
            y: rect.y,
        },
        size: Size {
            width: rect.width,
            height: rect.height,
        },
        visible,
        always_on_top: matches!(
            state,
            EdgeUiState::RailVisible | EdgeUiState::CardVisible | EdgeUiState::Pinned
        ),
        view_state,
        placement,
    }
}

fn native_layout_changed(previous: WindowLayout, next: WindowLayout) -> bool {
    previous.position != next.position
        || previous.size != next.size
        || previous.visible != next.visible
        || previous.always_on_top != next.always_on_top
        || previous.placement != next.placement
}

fn hidden_rect(
    placement: EdgePlacement,
    work_area: MonitorWorkArea,
    scale: MonitorScale,
    provider_count: u8,
) -> Rect {
    let rail = visible_rect_scaled_for_provider_count(
        placement,
        work_area,
        scale,
        EdgeUiState::RailVisible,
        provider_count,
    );
    match placement {
        EdgePlacement::Right => Rect {
            x: coordinate_end(work_area.x, work_area.width),
            ..rail
        },
        EdgePlacement::Left => Rect {
            x: work_area
                .x
                .checked_sub(u32_to_i32(rail.width))
                .unwrap_or_else(|| coordinate_end(work_area.x, work_area.width)),
            ..rail
        },
        EdgePlacement::Top => Rect {
            y: work_area
                .y
                .checked_sub(u32_to_i32(rail.height))
                .unwrap_or_else(|| coordinate_end(work_area.y, work_area.height)),
            ..rail
        },
    }
}

fn coordinate_end(start: i32, length: u32) -> i32 {
    (i64::from(start) + i64::from(length)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn rail_logical_extent(provider_count: u8) -> u32 {
    30 + 80 * u32::from(provider_count.clamp(1, 3))
}

fn control_logical_extent(provider_count: u8) -> u32 {
    rail_logical_extent(provider_count) + SETTINGS_CONTROL_EXTENT
}

fn subtract_from_end(start: i32, length: u32, amount: u32) -> i32 {
    (i64::from(start) + i64::from(length) - i64::from(amount))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn centered_start(start: i32, available: u32, extent: u32) -> i32 {
    let offset = available.saturating_sub(extent) / 2;
    (i64::from(start) + i64::from(offset)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::models::ProviderId;
    use crate::desktop::settings::EdgePlacement;
    use std::time::Duration;

    fn work() -> MonitorWorkArea {
        MonitorWorkArea::new(0, 0, 1920, 1040).unwrap()
    }

    #[test]
    fn rail_extent_tracks_enabled_provider_count() {
        assert_eq!(rail_logical_extent(1), 110);
        assert_eq!(rail_logical_extent(2), 190);
        assert_eq!(rail_logical_extent(3), 270);
        assert_eq!(control_logical_extent(1), 180);
        assert_eq!(control_logical_extent(2), 260);
        assert_eq!(control_logical_extent(3), 340);
    }

    #[test]
    fn provider_count_sizes_each_collapsed_placement_without_changing_expanded_bounds() {
        for (provider_count, extent) in [(1, 180), (2, 260), (3, 340)] {
            let side_rail = window_layout_scaled(
                EdgePlacement::Right,
                work(),
                MonitorScale::ONE,
                EdgeUiState::RailVisible,
                None,
                provider_count,
            );
            let top_rail = window_layout_scaled(
                EdgePlacement::Top,
                work(),
                MonitorScale::ONE,
                EdgeUiState::RailVisible,
                None,
                provider_count,
            );
            let side_card = window_layout_scaled(
                EdgePlacement::Right,
                work(),
                MonitorScale::ONE,
                EdgeUiState::CardVisible,
                Some(ProviderId::Claude),
                provider_count,
            );
            let top_card = window_layout_scaled(
                EdgePlacement::Top,
                work(),
                MonitorScale::ONE,
                EdgeUiState::CardVisible,
                Some(ProviderId::Claude),
                provider_count,
            );

            assert_eq!((side_rail.size.width, side_rail.size.height), (70, extent));
            assert_eq!((top_rail.size.width, top_rail.size.height), (extent, 70));
            assert_eq!((side_card.size.width, side_card.size.height), (370, 360));
            assert_eq!((top_card.size.width, top_card.size.height), (340, 430));
        }
    }

    fn idle(cursor: Option<Point>) -> EdgeInput {
        EdgeInput {
            cursor,
            placement: EdgePlacement::Right,
            work_area: work(),
            scale: MonitorScale::ONE,
            provider_count: 3,
            foreground_fullscreen: false,
            always_show_over_fullscreen: false,
            interaction: None,
        }
    }

    fn event(interaction: EdgeInteraction) -> EdgeInput {
        EdgeInput {
            cursor: None,
            interaction: Some(interaction),
            ..idle(None)
        }
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn reveal(machine: &mut EdgeMachine) {
        assert!(machine
            .advance(ms(0), idle(Some(Point { x: 1919, y: 520 })))
            .is_empty());
        assert!(!machine
            .advance(ms(100), idle(Some(Point { x: 1919, y: 520 })))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::RailVisible);
    }

    fn select(machine: &mut EdgeMachine, now: u64, provider: ProviderId) {
        assert!(!machine
            .advance(ms(now), event(EdgeInteraction::SelectProvider(provider)))
            .is_empty());
    }

    #[test]
    fn activation_zones_follow_each_supported_work_area_edge() {
        assert!(activation_zone(EdgePlacement::Right, work()).contains(Point { x: 1919, y: 520 }));
        assert!(activation_zone(EdgePlacement::Left, work()).contains(Point { x: 0, y: 520 }));
        assert!(activation_zone(EdgePlacement::Top, work()).contains(Point { x: 960, y: 0 }));
        assert!(!activation_zone(EdgePlacement::Right, work()).contains(Point { x: 1890, y: 520 }));
    }

    #[test]
    fn work_area_accepts_the_maximum_representable_half_open_bounds() {
        let work = MonitorWorkArea::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX).unwrap();

        assert_eq!(work.x(), i32::MIN);
        assert_eq!(work.y(), i32::MIN);
        assert_eq!(work.width(), u32::MAX);
        assert_eq!(work.height(), u32::MAX);
        assert_eq!(work.right(), i32::MAX);
        assert_eq!(work.bottom(), i32::MAX);
    }

    #[test]
    fn work_area_rejects_zero_extents_and_nonrepresentable_ends() {
        assert_eq!(
            MonitorWorkArea::new(0, 0, 0, 1),
            Err(MonitorWorkAreaError::ZeroExtent)
        );
        assert_eq!(
            MonitorWorkArea::new(0, 0, 1, 0),
            Err(MonitorWorkAreaError::ZeroExtent)
        );
        assert_eq!(
            MonitorWorkArea::new(i32::MAX, 0, 1, 1),
            Err(MonitorWorkAreaError::HorizontalOverflow)
        );
        assert_eq!(
            MonitorWorkArea::new(0, i32::MAX, 1, 1),
            Err(MonitorWorkAreaError::VerticalOverflow)
        );
    }

    #[test]
    fn negative_coordinate_monitor_geometry_stays_inside_the_work_area() {
        let work = MonitorWorkArea::new(-1920, -200, 1600, 900).unwrap();

        assert!(activation_zone(EdgePlacement::Right, work).contains(Point { x: -321, y: 250 }));
        assert!(activation_zone(EdgePlacement::Left, work).contains(Point { x: -1920, y: 250 }));
        assert!(activation_zone(EdgePlacement::Top, work).contains(Point { x: -1120, y: -200 }));

        for placement in [
            EdgePlacement::Right,
            EdgePlacement::Left,
            EdgePlacement::Top,
        ] {
            let rail = visible_rect(placement, work, EdgeUiState::RailVisible);
            let card = visible_rect(placement, work, EdgeUiState::CardVisible);
            assert!(work.rect().contains_rect(rail), "{placement:?} rail");
            assert!(work.rect().contains_rect(card), "{placement:?} card");
        }
    }

    #[test]
    fn visible_geometry_has_binding_dimensions_and_expands_inward() {
        assert_eq!(
            visible_rect(EdgePlacement::Right, work(), EdgeUiState::RailVisible),
            Rect {
                x: 1850,
                y: 350,
                width: 70,
                height: 340,
            }
        );
        assert_eq!(
            visible_rect(EdgePlacement::Left, work(), EdgeUiState::RailVisible),
            Rect {
                x: 0,
                y: 350,
                width: 70,
                height: 340,
            }
        );
        assert_eq!(
            visible_rect(EdgePlacement::Top, work(), EdgeUiState::RailVisible),
            Rect {
                x: 790,
                y: 0,
                width: 340,
                height: 70,
            }
        );
        assert_eq!(
            visible_rect(EdgePlacement::Right, work(), EdgeUiState::CardVisible),
            Rect {
                x: 1550,
                y: 340,
                width: 370,
                height: 360,
            }
        );
        assert_eq!(
            visible_rect(EdgePlacement::Left, work(), EdgeUiState::CardVisible),
            Rect {
                x: 0,
                y: 340,
                width: 370,
                height: 360,
            }
        );
        assert_eq!(
            visible_rect(EdgePlacement::Top, work(), EdgeUiState::CardVisible),
            Rect {
                x: 790,
                y: 0,
                width: 340,
                height: 430,
            }
        );
    }

    #[test]
    fn logical_geometry_scales_once_at_common_windows_dpi_values() {
        let cases = [
            (1.0, 96, 70, 340, 370, 360, 28),
            (1.25, 120, 88, 425, 463, 450, 35),
            (1.5, 144, 105, 510, 555, 540, 42),
            (2.0, 192, 140, 680, 740, 720, 56),
        ];

        for (factor, dpi, rail_width, rail_height, card_width, card_height, activation) in cases {
            let scale = MonitorScale::try_from_scale_factor(factor).unwrap();
            assert_eq!(scale.effective_dpi(), dpi);

            let rail = visible_rect_scaled(
                EdgePlacement::Right,
                work(),
                scale,
                EdgeUiState::RailVisible,
            );
            assert_eq!((rail.width, rail.height), (rail_width, rail_height));

            let card = visible_rect_scaled(
                EdgePlacement::Right,
                work(),
                scale,
                EdgeUiState::CardVisible,
            );
            assert_eq!((card.width, card.height), (card_width, card_height));

            let zone = activation_zone_scaled(EdgePlacement::Right, work(), scale);
            assert_eq!(zone.width, activation);
            assert_eq!(zone.x, work().right() - i32::try_from(activation).unwrap());
        }
    }

    #[test]
    fn compact_and_expanded_native_bounds_match_the_css_viewport_contract() {
        for scale in [
            MonitorScale::ONE,
            MonitorScale::from_effective_dpi(144, 144).unwrap(),
        ] {
            let side_rail = visible_rect_scaled(
                EdgePlacement::Right,
                work(),
                scale,
                EdgeUiState::RailVisible,
            );
            let left_rail =
                visible_rect_scaled(EdgePlacement::Left, work(), scale, EdgeUiState::RailVisible);
            let top_rail =
                visible_rect_scaled(EdgePlacement::Top, work(), scale, EdgeUiState::RailVisible);
            let right_card = visible_rect_scaled(
                EdgePlacement::Right,
                work(),
                scale,
                EdgeUiState::CardVisible,
            );
            let left_card =
                visible_rect_scaled(EdgePlacement::Left, work(), scale, EdgeUiState::CardVisible);
            let top_card =
                visible_rect_scaled(EdgePlacement::Top, work(), scale, EdgeUiState::CardVisible);

            assert_eq!(side_rail.width, scale.logical_to_physical(70));
            assert_eq!(side_rail.height, scale.logical_to_physical(340));
            assert_eq!(left_rail.width, side_rail.width);
            assert_eq!(left_rail.height, side_rail.height);
            assert_eq!(top_rail.width, scale.logical_to_physical(340));
            assert_eq!(top_rail.height, scale.logical_to_physical(70));
            assert_eq!(right_card.width, scale.logical_to_physical(370));
            assert_eq!(right_card.height, scale.logical_to_physical(360));
            assert_eq!(left_card.width, right_card.width);
            assert_eq!(left_card.height, right_card.height);
            assert_eq!(top_card.width, scale.logical_to_physical(340));
            assert_eq!(top_card.height, scale.logical_to_physical(430));
            assert_eq!(
                right_card.x + i32::try_from(right_card.width).unwrap(),
                work().right()
            );
            assert_eq!(left_card.x, work().x());
            assert_eq!(top_card.y, work().y());
        }
    }

    #[test]
    fn edge_view_state_rejects_unknown_fields_and_incoherent_provider_shapes() {
        let invalid_payloads = [
            r#"{"visibility":"rail","placement":"right","provider":null,"extra":true}"#,
            r#"{"visibility":"hidden","placement":"right"}"#,
            r#"{"visibility":"card","placement":"right","provider":null}"#,
            r#"{"visibility":"pinned","placement":"left","provider":null}"#,
            r#"{"visibility":"suppressed","placement":"top","provider":"claude"}"#,
            r#"{"visibility":"hidden","placement":"right","provider":"github"}"#,
            r#"{"visibility":"rail","placement":"left","provider":"codex"}"#,
            r#"{"visibility":"open","placement":"right","provider":null}"#,
            r#"{"visibility":"rail","placement":"bottom","provider":null}"#,
            r#"{"visibility":"card","placement":"top","provider":"other"}"#,
        ];

        for payload in invalid_payloads {
            assert!(
                serde_json::from_str::<EdgeViewState>(payload).is_err(),
                "invalid edge payload was accepted: {payload}"
            );
        }
    }

    #[test]
    fn edge_view_state_json_is_exact_for_every_coherent_enum_combination() {
        let placements = [
            (EdgePlacement::Right, "right"),
            (EdgePlacement::Left, "left"),
            (EdgePlacement::Top, "top"),
        ];
        let collapsed_visibilities = [
            (EdgeUiState::Suppressed, "suppressed"),
            (EdgeUiState::Hidden, "hidden"),
            (EdgeUiState::RailVisible, "rail"),
        ];
        let expanded_visibilities = [
            (EdgeUiState::CardVisible, "card"),
            (EdgeUiState::Pinned, "pinned"),
        ];
        let providers = [
            (ProviderId::GitHub, "github"),
            (ProviderId::Codex, "codex"),
            (ProviderId::Claude, "claude"),
        ];

        for (placement, placement_json) in placements {
            for (visibility, visibility_json) in collapsed_visibilities {
                let state = EdgeViewState {
                    visibility,
                    placement,
                    provider: None,
                };
                let expected = format!(
                    r#"{{"visibility":"{visibility_json}","placement":"{placement_json}","provider":null}}"#
                );
                assert_eq!(serde_json::to_string(&state).unwrap(), expected);
                assert_eq!(
                    serde_json::from_str::<EdgeViewState>(&expected).unwrap(),
                    state
                );
            }

            for (visibility, visibility_json) in expanded_visibilities {
                for (provider, provider_json) in providers {
                    let state = EdgeViewState {
                        visibility,
                        placement,
                        provider: Some(provider),
                    };
                    let expected = format!(
                        r#"{{"visibility":"{visibility_json}","placement":"{placement_json}","provider":"{provider_json}"}}"#
                    );
                    assert_eq!(serde_json::to_string(&state).unwrap(), expected);
                    assert_eq!(
                        serde_json::from_str::<EdgeViewState>(&expected).unwrap(),
                        state
                    );
                }
            }
        }
    }

    #[test]
    fn monitor_scale_rejects_non_finite_mismatched_and_out_of_range_values() {
        assert!(MonitorScale::try_from_scale_factor(f64::NAN).is_err());
        assert!(MonitorScale::try_from_scale_factor(f64::INFINITY).is_err());
        assert!(MonitorScale::try_from_scale_factor(0.49).is_err());
        assert!(MonitorScale::try_from_scale_factor(8.01).is_err());
        assert!(MonitorScale::from_effective_dpi(120, 144).is_err());
        assert!(MonitorScale::from_effective_dpi(0, 0).is_err());
    }

    #[test]
    fn geometry_caps_to_a_work_area_smaller_than_the_preferred_surface() {
        let small = MonitorWorkArea::new(-200, -100, 200, 100).unwrap();

        for placement in [
            EdgePlacement::Right,
            EdgePlacement::Left,
            EdgePlacement::Top,
        ] {
            for state in [EdgeUiState::RailVisible, EdgeUiState::CardVisible] {
                let rect = visible_rect(placement, small, state);
                assert!(small.rect().contains_rect(rect));
                assert!(rect.width <= small.width);
                assert!(rect.height <= small.height);
            }
        }
    }

    #[test]
    fn hidden_geometry_is_fully_outside_the_work_area_for_every_placement() {
        for placement in [
            EdgePlacement::Right,
            EdgePlacement::Left,
            EdgePlacement::Top,
        ] {
            let hidden = window_layout(placement, work(), EdgeUiState::Hidden, None);
            assert!(!hidden.visible);
            assert!(!work().rect().intersects(hidden.rect()));
        }
    }

    #[test]
    fn reveal_requires_one_hundred_milliseconds_of_continuous_presence() {
        let mut machine = EdgeMachine::new();
        assert!(machine
            .advance(ms(0), idle(Some(Point { x: 1919, y: 520 })))
            .is_empty());
        assert!(machine
            .advance(ms(99), idle(Some(Point { x: 1919, y: 520 })))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::Hidden);

        let effects = machine.advance(ms(100), idle(Some(Point { x: 1919, y: 520 })));
        assert_eq!(machine.state(), EdgeUiState::RailVisible);
        assert_eq!(effects.len(), 2);
    }

    #[test]
    fn leaving_the_activation_zone_resets_reveal_dwell() {
        let mut machine = EdgeMachine::new();
        machine.advance(ms(0), idle(Some(Point { x: 1919, y: 520 })));
        machine.advance(ms(99), idle(Some(Point { x: 1800, y: 520 })));
        machine.advance(ms(100), idle(Some(Point { x: 1919, y: 520 })));
        assert!(machine
            .advance(ms(199), idle(Some(Point { x: 1919, y: 520 })))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::Hidden);
        assert!(!machine
            .advance(ms(200), idle(Some(Point { x: 1919, y: 520 })))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::RailVisible);
    }

    #[test]
    fn changing_selected_work_area_resets_reveal_dwell() {
        let mut machine = EdgeMachine::new();
        machine.advance(ms(0), idle(Some(Point { x: 1919, y: 520 })));
        let shifted_work = MonitorWorkArea::new(1920, 0, 1920, 1040).unwrap();
        let shifted = EdgeInput {
            cursor: Some(Point { x: 3839, y: 520 }),
            work_area: shifted_work,
            ..idle(None)
        };

        machine.advance(ms(99), shifted);
        machine.advance(ms(198), shifted);
        assert_eq!(machine.state(), EdgeUiState::Hidden);
        machine.advance(ms(199), shifted);
        assert_eq!(machine.state(), EdgeUiState::RailVisible);
    }

    #[test]
    fn changing_layout_context_invalidates_the_old_window_safe_region() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::GitHub);
        let shifted = EdgeInput {
            cursor: None,
            work_area: MonitorWorkArea::new(1920, 0, 1920, 1040).unwrap(),
            ..idle(None)
        };

        machine.advance(ms(102), shifted);
        machine.advance(ms(521), shifted);
        assert_eq!(machine.state(), EdgeUiState::CardVisible);
        machine.advance(ms(522), shifted);
        assert_eq!(machine.state(), EdgeUiState::Hidden);
    }

    #[test]
    fn cursor_near_a_different_monitor_does_not_start_reveal() {
        let mut machine = EdgeMachine::new();
        let other_monitor_edge = Point { x: -1, y: 520 };
        assert!(machine
            .advance(ms(0), idle(Some(other_monitor_edge)))
            .is_empty());
        assert!(machine
            .advance(ms(100), idle(Some(other_monitor_edge)))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::Hidden);
    }

    #[test]
    fn provider_hover_opens_card_and_switches_provider_without_hiding() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::GitHub);
        assert_eq!(machine.state(), EdgeUiState::CardVisible);
        assert_eq!(machine.selected_provider(), Some(ProviderId::GitHub));

        let effects = machine.advance(
            ms(102),
            event(EdgeInteraction::SelectProvider(ProviderId::Codex)),
        );
        assert_eq!(machine.state(), EdgeUiState::CardVisible);
        assert_eq!(machine.selected_provider(), Some(ProviderId::Codex));
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], EdgeEffect::EmitView(_)));
        assert!(effects.iter().all(|effect| !matches!(
            effect,
            EdgeEffect::EmitView(EdgeViewState {
                visibility: EdgeUiState::Hidden,
                ..
            })
        )));
    }

    #[test]
    fn close_grace_expires_at_four_hundred_twenty_milliseconds() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::Claude);
        machine.advance(ms(102), event(EdgeInteraction::LeaveSafeRegion));

        let outside = idle(Some(Point { x: 1800, y: 520 }));
        assert!(machine.advance(ms(521), outside).is_empty());
        assert_eq!(machine.state(), EdgeUiState::CardVisible);

        let effects = machine.advance(ms(522), outside);
        assert_eq!(machine.state(), EdgeUiState::Hidden);
        assert_eq!(effects.len(), 2);
    }

    #[test]
    fn reentering_safe_region_cancels_close_grace() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::Claude);
        machine.advance(ms(102), event(EdgeInteraction::LeaveSafeRegion));
        machine.advance(ms(521), event(EdgeInteraction::EnterSafeRegion));

        assert!(machine
            .advance(ms(900), idle(Some(Point { x: 1800, y: 520 })))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::CardVisible);
    }

    #[test]
    fn pinned_state_ignores_pointer_exit_and_close_grace() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::Codex);
        machine.advance(
            ms(102),
            event(EdgeInteraction::TogglePin(ProviderId::Codex)),
        );
        machine.advance(ms(103), event(EdgeInteraction::LeaveSafeRegion));

        assert!(machine
            .advance(ms(10_000), idle(Some(Point { x: 1800, y: 520 })))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::Pinned);
    }

    #[test]
    fn clicking_the_pinned_provider_again_unpins_without_losing_selection() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::Codex);
        machine.advance(
            ms(102),
            event(EdgeInteraction::TogglePin(ProviderId::Codex)),
        );

        let effects = machine.advance(
            ms(103),
            event(EdgeInteraction::TogglePin(ProviderId::Codex)),
        );
        assert_eq!(machine.state(), EdgeUiState::CardVisible);
        assert_eq!(machine.selected_provider(), Some(ProviderId::Codex));
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], EdgeEffect::EmitView(_)));
    }

    #[test]
    fn pinned_selection_ignores_hover_and_atomically_switches_on_click() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        machine.advance(ms(1), event(EdgeInteraction::TogglePin(ProviderId::Claude)));
        assert_eq!(machine.state(), EdgeUiState::Pinned);
        assert_eq!(machine.selected_provider(), Some(ProviderId::Claude));

        let hover = machine.advance(
            ms(2),
            event(EdgeInteraction::SelectProvider(ProviderId::Codex)),
        );
        assert!(hover.is_empty());
        assert_eq!(machine.state(), EdgeUiState::Pinned);
        assert_eq!(machine.selected_provider(), Some(ProviderId::Claude));

        let switch = machine.advance(ms(3), event(EdgeInteraction::TogglePin(ProviderId::Codex)));
        assert_eq!(machine.state(), EdgeUiState::Pinned);
        assert_eq!(machine.selected_provider(), Some(ProviderId::Codex));
        assert_eq!(
            switch
                .into_iter()
                .filter_map(|effect| match effect {
                    EdgeEffect::EmitView(view) => Some(view),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![EdgeViewState {
                visibility: EdgeUiState::Pinned,
                placement: EdgePlacement::Right,
                provider: Some(ProviderId::Codex),
            }],
        );

        machine.advance(ms(4), event(EdgeInteraction::TogglePin(ProviderId::Codex)));
        assert_eq!(machine.state(), EdgeUiState::CardVisible);
        assert_eq!(machine.selected_provider(), Some(ProviderId::Codex));
    }

    #[test]
    fn clearing_hover_closes_the_card_to_the_rail() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::GitHub);

        let effects = machine.advance(ms(102), event(EdgeInteraction::ClearProvider));
        assert_eq!(machine.state(), EdgeUiState::RailVisible);
        assert_eq!(machine.selected_provider(), None);
        assert_eq!(effects.len(), 2);
    }

    #[test]
    fn outside_click_unpins_and_keeps_selected_provider_for_focus_restore() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::GitHub);
        machine.advance(
            ms(102),
            event(EdgeInteraction::TogglePin(ProviderId::GitHub)),
        );

        let effects = machine.advance(ms(103), event(EdgeInteraction::OutsideClick));
        assert_eq!(machine.state(), EdgeUiState::CardVisible);
        assert_eq!(machine.selected_provider(), Some(ProviderId::GitHub));
        assert_eq!(
            effects.last(),
            Some(&EdgeEffect::EmitView(EdgeViewState {
                visibility: EdgeUiState::CardVisible,
                placement: EdgePlacement::Right,
                provider: Some(ProviderId::GitHub),
            }))
        );
    }

    #[test]
    fn fullscreen_suppresses_and_override_returns_to_hidden() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        let mut fullscreen = idle(Some(Point { x: 1919, y: 520 }));
        fullscreen.foreground_fullscreen = true;

        assert!(!machine.advance(ms(101), fullscreen).is_empty());
        assert_eq!(machine.state(), EdgeUiState::Suppressed);

        fullscreen.always_show_over_fullscreen = true;
        let effects = machine.advance(ms(102), fullscreen);
        assert_eq!(machine.state(), EdgeUiState::Hidden);
        assert_eq!(
            effects,
            vec![EdgeEffect::EmitView(EdgeViewState {
                visibility: EdgeUiState::Hidden,
                placement: EdgePlacement::Right,
                provider: None,
            })]
        );
    }

    #[test]
    fn escape_closes_card_to_rail_then_hides_rail() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::Claude);
        machine.advance(
            ms(102),
            event(EdgeInteraction::TogglePin(ProviderId::Claude)),
        );

        assert!(!machine
            .advance(ms(103), event(EdgeInteraction::Escape))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::RailVisible);
        assert_eq!(machine.selected_provider(), None);

        assert!(!machine
            .advance(ms(104), event(EdgeInteraction::Escape))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::Hidden);
    }

    #[test]
    fn escape_closes_an_unpinned_card_to_the_rail() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        select(&mut machine, 101, ProviderId::Claude);

        assert!(!machine
            .advance(ms(102), event(EdgeInteraction::Escape))
            .is_empty());
        assert_eq!(machine.state(), EdgeUiState::RailVisible);
        assert_eq!(machine.selected_provider(), None);
    }

    #[test]
    fn unchanged_ticks_emit_no_redundant_effects() {
        let mut machine = EdgeMachine::new();
        reveal(&mut machine);
        assert!(machine
            .advance(ms(101), idle(Some(Point { x: 1919, y: 520 })))
            .is_empty());
    }
}
