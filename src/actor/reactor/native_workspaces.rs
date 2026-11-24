use std::collections::VecDeque;

use dispatchr::{queue, time::Time};
use rift_protocol::{LayoutCommand, WorkspaceSelector};

use super::*;
use crate::model::VirtualWorkspaceId;
use crate::model::virtual_workspace::WorkspaceContext;
use crate::sys::{dispatch::DispatchExt, native_spaces};

const CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) fn is_native_workspace_operation(cmd: &LayoutCommand) -> bool {
    matches!(cmd, LayoutCommand::NextWorkspace(_) | LayoutCommand::PrevWorkspace(_)
        | LayoutCommand::SwitchToWorkspace(_) | LayoutCommand::SwitchToLastWorkspace
        | LayoutCommand::MoveWindowToWorkspace { .. } | LayoutCommand::CreateWorkspace)
}

#[derive(Debug, Default)]
pub(super) struct NativeWorkspaces {
    previous: HashMap<SpaceId, SpaceId>,
    switches: HashMap<String, Switch>,
    queued_navigation: VecDeque<(LayoutCommand, Instant)>,
    moves: Vec<Move>,
    drop_replies: HashMap<WindowId, std::sync::mpsc::SyncSender<bool>>,
    creates: HashMap<SpaceId, (String, Instant)>,
    timer_pending: bool,
}

#[derive(Debug)]
struct Switch {
    start: SpaceId,
    target: SpaceId,
    focus: Option<WindowId>,
    overview: Option<Event>,
    deadline: Instant,
}

#[derive(Debug)]
struct Move {
    target: SpaceId,
    windows: Vec<(WindowId, WindowServerId)>,
    follow: bool,
    focus: Option<WindowId>,
    drop: Option<OverviewDrop>,
    direction: Option<Direction>,
    failed: bool,
    deadline: Instant,
}

impl NativeWorkspaces {
    pub(super) fn on_tick(&mut self) {
        self.timer_pending = false;
    }
}

impl Reactor {
    pub(super) fn native_workspaces_enabled(&self) -> bool {
        self.layout_manager.layout_engine.workspaces().native
    }

    pub(super) fn native_desktop_screen(&self, space: SpaceId) -> Option<&ScreenInfo> {
        if !self.native_workspaces_enabled() {
            return self.space_state.screen_by_space(space);
        }
        self.space_state.screen_by_space(space).or_else(|| {
            self.space_state.screens.iter().find(|screen| {
                self.space_state
                    .display_space_ids
                    .get(&screen.display_uuid)
                    .is_some_and(|ids| ids.contains(&space))
            })
        })
    }

    pub(super) fn sync_native_workspaces(&mut self, previous: &[ScreenInfo]) {
        let displays: Vec<_> = self
            .screens_in_physical_order()
            .into_iter()
            .map(|screen| {
                let spaces = self
                    .space_state
                    .display_space_ids
                    .get(&screen.display_uuid)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|space| window_server::space_is_user(space.get()))
                    .collect::<Vec<_>>();
                (screen.display_uuid.clone(), screen.frame.size, spaces)
            })
            .collect();
        let catalog: Vec<_> = displays.iter().map(|(_, _, spaces)| spaces.clone()).collect();
        let engine = &mut self.layout_manager.layout_engine;
        engine.workspaces_mut().sync_native_catalog(&catalog);
        for (display, size, spaces) in displays {
            for space in spaces {
                if engine.display_uuid_for_space(space) != Some(display.as_str()) {
                    engine.update_space_display(space, Some(&display));
                }
                let workspace = engine.workspaces().active_workspace(space).unwrap();
                if engine.workspaces()[workspace].layout_state.active_size() != Some(size) {
                    let _ = engine.handle_event(
                        &mut self.state.windows,
                        LayoutEvent::SpaceExposed(space, size),
                    );
                }
            }
        }
        for screen in &self.space_state.screens {
            let old = previous.iter().find(|old| old.display_uuid == screen.display_uuid);
            if let (Some(from), Some(to)) = (old.and_then(|s| s.space), screen.space)
                && from != to
                && engine.workspaces().same_native_display(from, to)
            {
                self.native_workspaces.previous.insert(to, from);
            }
        }
    }

    pub(super) fn native_workspace_command(
        &mut self,
        changes: &mut Changes,
        cmd: &LayoutCommand,
        space: SpaceId,
    ) -> Option<bool> {
        if (self.refreshes_blocked() || !self.native_workspaces.switches.is_empty())
            && is_native_workspace_operation(cmd)
        {
            if self.native_workspaces.queued_navigation.len() >= 16 {
                return Some(false);
            }
            self.native_workspaces.queued_navigation.push_back((cmd.clone(), Instant::now() + CONFIRMATION_TIMEOUT));
            self.schedule_native_workspace_tick();
            return Some(true);
        }
        let store = self.layout_manager.layout_engine.workspaces();
        let context = WorkspaceContext::from(space);
        let current = store.active_workspace(space);
        let skip_empty = |skip_empty| {
            let direction = match cmd {
                LayoutCommand::NextWorkspace(_) => Direction::Right,
                _ => Direction::Left,
            };
            store.step_workspace_matching(context, current?, direction, |id| {
                skip_empty != Some(true) || self.native_workspace_has_windows(id)
            })
        };
        let target = match cmd {
            LayoutCommand::NextWorkspace(skip) | LayoutCommand::PrevWorkspace(skip) => {
                skip_empty(*skip)
                    .filter(|id| Some(*id) != current)
                    .and_then(|id| store.workspace_space(id))
            }
            LayoutCommand::SwitchToWorkspace(index) => {
                let target = store.resolve_workspace(context, &WorkspaceSelector::Index(*index));
                if target.is_some_and(|target| target.space == space) {
                    store
                        .settings
                        .workspace_auto_back_and_forth
                        .then(|| self.native_previous(space))
                        .flatten()
                } else {
                    target.map(|target| target.space)
                }
            }
            LayoutCommand::SwitchToLastWorkspace => self.native_previous(space),
            LayoutCommand::MoveWindowToWorkspace { workspace, follow, window_id } => {
                let window = match window_id {
                    Some(index) => store.resolve_window_index(
                        &self.state.windows,
                        Some(space),
                        |space| self.is_space_active(space),
                        *index,
                    ),
                    None => self.layout_manager.layout_engine.focused_window(),
                };
                let result = (|| {
                    let window =
                        window.filter(|&wid| self.state.windows.can_move_assigned_window(wid))?;
                    let owner = self.state.windows.workspace_for_window(window)?;
                    let origin = store.workspace_space(owner)?;
                    let destination = match workspace {
                        WorkspaceSelector::Name(name) if name == "next" || name == "prev" => {
                            let direction = if name == "next" {
                                Direction::Right
                            } else {
                                Direction::Left
                            };
                            store.step_workspace(
                                &self.state.windows,
                                origin,
                                owner,
                                None,
                                direction,
                            )
                        }
                        selector => {
                            store.resolve_workspace(context, selector).map(|target| target.id)
                        }
                    }
                    .filter(|&id| id != owner)?;
                    let target = store.workspace_space(destination)?;
                    Some((window, target))
                })();
                return Some(result.is_some_and(|(window, target)| {
                    self.submit_native_move(&[window], target, *follow, Some(window), None)
                }));
            }
            LayoutCommand::CreateWorkspace => return Some(self.create_native_workspace(space)),
            _ => return None,
        };
        Some(target.is_some_and(|target| {
            self.begin_native_workspace_switch(changes, space, target, None, None)
        }))
    }

    fn native_workspace_has_windows(&self, id: VirtualWorkspaceId) -> bool {
        if self.state.windows.workspace_window_count(id) != 0 {
            return true;
        }
        let Some(space) = self.layout_manager.layout_engine.workspaces().workspace_space(id) else {
            return false;
        };
        // Unvisited Spaces can already contain windows that AX has not discovered yet.
        window_server::try_space_window_list_for_connection(&[space.get()], 0, true).is_none_or(
            |ids| {
                ids.into_iter().map(WindowServerId::new).any(|id| {
                    self.state.windows.tracked_window_id(id).map_or_else(
                        || window_server::app_window_suitability(id) == Some(true),
                        |wid| self.state.windows.is_admitted(wid),
                    )
                })
            },
        )
    }

    fn native_previous(&self, space: SpaceId) -> Option<SpaceId> {
        let previous = *self.native_workspaces.previous.get(&space)?;
        let store = self.layout_manager.layout_engine.workspaces();
        (self.native_desktop_screen(previous).is_some()
            && (store.effective_mode() == crate::common::config::WorkspaceMode::Global
                || store.same_native_display(space, previous)))
        .then_some(previous)
    }

    pub(super) fn create_native_workspace(&mut self, space: SpaceId) -> bool {
        if self.refreshes_blocked() {
            return false;
        }
        let Some(display_uuid) =
            self.native_desktop_screen(space).map(|screen| screen.display_uuid.clone())
        else {
            return false;
        };
        let Some(id) = native_spaces::create(&display_uuid) else {
            warn!(%display_uuid, "SkyLight did not return a new desktop");
            return false;
        };
        self.native_workspaces
            .creates
            .insert(id, (display_uuid, Instant::now() + CONFIRMATION_TIMEOUT));
        self.request_space_snapshot();
        self.schedule_native_workspace_tick();
        true
    }

    pub(super) fn begin_native_workspace_switch(
        &mut self,
        changes: &mut Changes,
        start: SpaceId,
        target: SpaceId,
        focus: Option<WindowId>,
        overview: Option<Event>,
    ) -> bool {
        if self.refreshes_blocked() || self.is_in_drag() {
            return false;
        }
        let Some(display) =
            self.native_desktop_screen(target).map(|screen| screen.display_uuid.clone())
        else {
            return false;
        };
        if !self.is_space_active(target)
            && self
                .native_workspaces
                .switches
                .get(&display)
                .is_none_or(|request| request.target != target)
            && !native_spaces::focus(&display, target)
        {
            warn!(?target, "SkyLight rejected desktop activation");
            return false;
        }
        self.store_current_floating_positions(start);
        self.retire_viewport_session();
        self.workspace_switch_manager
            .start_workspace_switch(target, WorkspaceSwitchOrigin::Manual);
        let pending = Switch {
            start,
            target,
            focus,
            overview,
            deadline: Instant::now() + CONFIRMATION_TIMEOUT,
        };
        if self.is_space_active(target) {
            self.native_workspaces.switches.remove(&display);
            self.finish_native_switch(changes, pending);
        } else {
            self.native_workspaces.switches.insert(display, pending);
            self.request_space_snapshot();
            self.schedule_native_workspace_tick();
        }
        true
    }

    fn finish_native_switch(&mut self, changes: &mut Changes, request: Switch) {
        let Switch {
            start, target, focus, overview, ..
        } = request;
        self.workspace_switch_manager
            .start_workspace_switch(target, WorkspaceSwitchOrigin::Manual);
        if start != target {
            self.native_workspaces.previous.insert(target, start);
            changes.workspace_changed(target);
        }
        if let Some(event) = overview {
            if let Some(tx) = &self.communication_manager.events_tx {
                tx.send(event);
            }
            return;
        }
        let store = self.layout_manager.layout_engine.workspaces();
        let Some(id) = store.active_workspace(target) else {
            return;
        };
        let focus_window = self.layout_manager.layout_engine.preferred_focus_for_workspace(
            &self.state.windows,
            target,
            id,
            focus,
        );
        if !store.same_native_display(start, target) && start != target {
            changes.display_warp = Some(target);
        }
        changes.arrange(Some(target), Presentation::WorkspaceSwitch);
        self.reconcile_layout_response(
            changes,
            layout::EventResponse {
                changed: true,
                workspace_space: Some(target),
                focus_window,
                ..Default::default()
            },
            Some(target),
        );
    }

    pub(super) fn native_move_pending(&self, window: WindowId) -> bool {
        self.native_workspaces
            .moves
            .iter()
            .any(|request| request.windows.iter().any(|(pending, _)| *pending == window))
    }

    pub(super) fn submit_native_overview_drop(
        &mut self, intent: OverviewDrop, target: SpaceId, reply: std::sync::mpsc::SyncSender<bool>,
    ) {
        let window = intent.window;
        if self.state.windows.can_move_assigned_window(window)
            && self.submit_native_move(&[window], target, false, None, Some(intent))
        {
            self.native_workspaces.drop_replies.insert(window, reply);
        } else {
            let _ = reply.send(false);
        }
    }

    fn finish_native_drop(&mut self, window: WindowId, changed: bool) {
        if let Some(reply) = self.native_workspaces.drop_replies.remove(&window) {
            let _ = reply.send(changed);
        }
    }

    pub(super) fn submit_native_move(
        &mut self,
        windows: &[WindowId],
        target: SpaceId,
        follow: bool,
        focus: Option<WindowId>,
        drop: Option<OverviewDrop>,
    ) -> bool {
        self.submit_native_move_with_direction(windows, target, follow, focus, drop, None)
    }

    pub(super) fn submit_native_directional_move(
        &mut self,
        windows: &[WindowId],
        target: SpaceId,
        direction: Direction,
    ) -> bool {
        let focus = self
            .layout_manager
            .layout_engine
            .focused_window()
            .filter(|window| windows.contains(window));
        self.submit_native_move_with_direction(
            windows,
            target,
            focus.is_some(),
            focus,
            None,
            Some(direction),
        )
    }

    fn submit_native_move_with_direction(
        &mut self,
        windows: &[WindowId],
        target: SpaceId,
        follow: bool,
        focus: Option<WindowId>,
        drop: Option<OverviewDrop>,
        direction: Option<Direction>,
    ) -> bool {
        if self.refreshes_blocked() || self.native_desktop_screen(target).is_none() {
            return false;
        }
        if windows.iter().any(|&wid| self.native_move_pending(wid)) {
            return false;
        }
        let mut ids = Vec::new();
        let members: Vec<_> = windows
            .iter()
            .filter_map(|&wid| {
                let id = self.state.windows.record(wid)?.window_server_id()?;
                let origin = self.assigned_space_for_window_id(wid)?;
                let membership = window_server::window_spaces(id);
                if origin == target {
                    return None;
                }
                if membership.as_slice() == [origin] {
                    ids.push(id);
                } else if membership.as_slice() != [target] {
                    return None;
                }
                Some((wid, id))
            })
            .collect();
        if members.is_empty() || members.len() != windows.len() {
            return false;
        }
        self.cancel_window_presentations(windows.to_vec());
        if !ids.is_empty() && !native_spaces::move_windows(&ids, target) {
            warn!(?target, ?ids, "SkyLight rejected window move");
            return false;
        }
        self.native_workspaces.moves.push(Move {
            target,
            windows: members,
            follow,
            focus,
            drop,
            direction,
            failed: false,
            deadline: Instant::now() + CONFIRMATION_TIMEOUT,
        });
        self.schedule_native_workspace_tick();
        true
    }

    pub(super) fn schedule_native_workspace_tick(&mut self) {
        if self.native_workspaces.timer_pending {
            return;
        }
        let Some(tx) = self.communication_manager.events_tx.clone() else {
            return;
        };
        self.native_workspaces.timer_pending = true;
        queue::main().after_f_s(Time::new_after(Time::NOW, 50_000_000), tx, |tx| {
            tx.send(Event::NativeWorkspacesTick)
        });
    }

    pub(super) fn advance_native_workspaces(&mut self, changes: &mut Changes, tick: bool) {
        let now = Instant::now();
        let switches = std::mem::take(&mut self.native_workspaces.switches);
        for (display, request) in switches {
            if !self.refreshes_blocked() && self.is_space_active(request.target) {
                self.finish_native_switch(changes, request);
            } else if now >= request.deadline {
                self.workspace_switch_manager.mark_workspace_switch_inactive();
                warn!(target = ?request.target, "Native desktop activation was not confirmed");
            } else {
                self.native_workspaces.switches.insert(display, request);
            }
        }
        self.native_workspaces.queued_navigation.retain(|(_, deadline)| *deadline > now);
        while tick
            && self.native_workspaces.switches.is_empty()
            && !self.refreshes_blocked()
            && let Some(space) = self.workspace_command_space()
            && let Some((cmd, _)) = self.native_workspaces.queued_navigation.pop_front()
        {
            self.native_workspace_command(changes, &cmd, space);
        }
        let creates = std::mem::take(&mut self.native_workspaces.creates);
        for (id, (display_uuid, deadline)) in creates {
            if deadline <= now {
                warn!(?id, %display_uuid, "Native desktop creation was not confirmed on requested display");
            } else if !self
                .space_state
                .display_space_ids
                .get(&display_uuid)
                .is_some_and(|ids| ids.contains(&id))
            {
                self.native_workspaces.creates.insert(id, (display_uuid, deadline));
            }
        }
        let moves = std::mem::take(&mut self.native_workspaces.moves);
        for mut request in moves {
            let origin = self.workspace_command_space();
            let moved_focus =
                request.focus.is_some_and(|window| Some(window) == self.main_window());
            let target = request.target;
            let mut failed = request.failed;
            let mut any = false;
            let expired = now >= request.deadline;
            request.windows.retain(|&(window, id)| {
                if self.state.windows.record(window).and_then(|record| record.window_server_id())
                    != Some(id)
                {
                    self.finish_native_drop(window, false);
                    failed = true;
                    return false;
                }
                if self.refreshes_blocked()
                    || window_server::window_spaces(id).as_slice() != [target]
                {
                    if expired {
                        self.finish_native_drop(window, false);
                    }
                    return true;
                }
                let committed = self.commit_native_move(
                    changes,
                    window,
                    target,
                    request.drop.as_ref(),
                    request.direction,
                );
                self.finish_native_drop(window, committed);
                any |= committed;
                failed |= !committed;
                false
            });
            let follow = request.follow;
            request.failed = failed;
            if request.windows.is_empty() && !failed && follow {
                if let Some(origin) = origin {
                    self.begin_native_workspace_switch(
                        changes,
                        origin,
                        target,
                        request.focus,
                        None,
                    );
                }
            } else if !request.windows.is_empty() && !expired {
                self.native_workspaces.moves.push(request);
            } else if failed || !request.windows.is_empty() {
                warn!(?target, windows = ?request.windows, "Some native window moves were not confirmed");
            }
            if any
                && moved_focus
                && !follow
                && let Some(origin) = origin.filter(|&origin| origin != target)
            {
                changes.fallback_focus = Some((origin, None));
            }
            if any || expired {
                self.request_space_snapshot();
            }
        }
        if tick
            && (!self.native_workspaces.switches.is_empty()
                || !self.native_workspaces.creates.is_empty())
        {
            self.request_space_snapshot();
        }
        if !self.native_workspaces.switches.is_empty()
            || !self.native_workspaces.moves.is_empty()
            || !self.native_workspaces.creates.is_empty()
            || !self.native_workspaces.queued_navigation.is_empty()
        {
            self.schedule_native_workspace_tick();
        }
    }

    fn commit_native_move(
        &mut self,
        changes: &mut Changes,
        window: WindowId,
        target: SpaceId,
        drop: Option<&OverviewDrop>,
        direction: Option<Direction>,
    ) -> bool {
        let source = self.assigned_space_for_window_id(window);
        let source_frame = source
            .and_then(|space| self.native_desktop_screen(space))
            .map(|screen| screen.frame);
        let engine = &mut self.layout_manager.layout_engine;
        let changed = drop.is_some_and(|intent| {
            engine.relocate_window_with_drop(&mut self.state.windows, intent)
        }) || engine.commit_native_workspace_move(
            &mut self.state.windows,
            window,
            target,
            direction,
        );
        if !changed {
            return false;
        }
        if let Some(id) =
            self.state.windows.record(window).and_then(|record| record.window_server_id())
        {
            self.state
                .windows
                .observe_native_space(id, target, self.is_space_active(target));
        }
        if let Some(source) = source {
            changes.move_between(source, target, Presentation::Animated);
        }
        let frame = drop.and_then(|intent| intent.frame).or_else(|| {
            if !self.layout_manager.layout_engine.is_window_floating(window) {
                return None;
            }
            let frame = self.state.windows.record(window)?.frame()?;
            let display_frame = self.native_desktop_screen(target)?.frame;
            Some(self.frame_on_display(window, frame, source_frame, display_frame))
        });
        if let Some(frame) = frame {
            if let Some(workspace) = self.state.windows.workspace_for_window(window) {
                self.layout_manager
                    .layout_engine
                    .store_floating_position(workspace, window, frame);
            }
            if self.is_space_active(target) {
                self.write_window_frame(window, frame, false, None);
            }
        }
        changes.refresh_menu = true;
        changes.overview_changed |= self.config.settings.ui.mission_control.enabled;
        true
    }

    pub(super) fn move_native_workspace_to_display(
        &mut self,
        changes: &mut Changes,
        source: SpaceId,
        target: SpaceId,
    ) {
        if source == target {
            return;
        }
        let Some(destination) = self.native_workspace_on_display(source, target) else {
            warn!(
                ?target,
                "The target display has no desktop at the source ordinal"
            );
            return;
        };
        let engine = &self.layout_manager.layout_engine;
        let Some(id) = engine.workspaces().active_workspace(source) else {
            return;
        };
        let windows = self.state.windows.workspace_windows(id);
        let focus = engine.preferred_focus_for_workspace(&self.state.windows, source, id, None);
        if windows.is_empty() {
            self.begin_native_workspace_switch(changes, source, destination, None, None);
        } else {
            self.submit_native_move(&windows, destination, true, focus, None);
        }
    }

    fn native_workspace_on_display(&self, source: SpaceId, target: SpaceId) -> Option<SpaceId> {
        let store = self.layout_manager.layout_engine.workspaces();
        let id: VirtualWorkspaceId = store.active_workspace(source)?;
        let index = (0..)
            .map_while(|index| store.native_desktop_at(source, index))
            .position(|space| space == source)?;
        debug_assert_eq!(store.workspace_space(id), Some(source));
        store.native_desktop_at(target, index)
    }
}
