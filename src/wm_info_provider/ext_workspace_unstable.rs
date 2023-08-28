use wayrs_client::object::ObjectId;
use wayrs_client::proxy::Proxy;
use wayrs_client::Connection;
use wayrs_client::{global::*, EventCtx};

use super::*;
use crate::state::State;

pub struct ExtWorkspaceUnstable {
    manager: ZextWorkspaceManagerV1,
    groups: Vec<Group>,
    known_outputs: Vec<WlOutput>,
}

#[derive(Debug)]
struct Group {
    group_handle: ZextWorkspaceGroupHandleV1,
    workspaces: Vec<Workspace>,
    outputs: Vec<ObjectId>,
}

#[derive(Debug)]
struct Workspace {
    workspace_handle: ZextWorkspaceHandleV1,
    name: Option<String>,
    is_focused: bool,
    is_urgent: bool,
}

impl ExtWorkspaceUnstable {
    pub fn bind(conn: &mut Connection<State>, globals: &Globals) -> Option<Self> {
        Some(Self {
            manager: globals.bind_with_cb(conn, 1, manager_cb).ok()?,
            groups: Vec::new(),
            known_outputs: Vec::new(),
        })
    }
}

impl WmInfoProvider for ExtWorkspaceUnstable {
    fn new_ouput(&mut self, _conn: &mut Connection<State>, output: WlOutput) {
        self.known_outputs.push(output);
    }

    fn output_removed(&mut self, _conn: &mut Connection<State>, output: WlOutput) {
        self.known_outputs.retain(|&o| o != output);
        for group in &mut self.groups {
            group.outputs.retain(|&id| id != output.id());
        }
    }

    fn get_tags(&self, output: WlOutput) -> Vec<Tag> {
        let Some(group) = self
            .groups
            .iter()
            .find(|g| g.outputs.contains(&output.id()))
        else {
            return Vec::new();
        };

        let mut tags: Vec<_> = group
            .workspaces
            .iter()
            .flat_map(|w| {
                let name = w.name.clone()?;
                Some(Tag {
                    name,
                    is_focused: w.is_focused,
                    is_active: true,
                    is_urgent: w.is_urgent,
                })
            })
            .collect();
        tags.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        tags
    }

    fn get_layout_name(&self, _output: WlOutput) -> Option<String> {
        None
    }

    fn get_mode_name(&self, _output: WlOutput) -> Option<String> {
        None
    }

    fn click_on_tag(
        &mut self,
        conn: &mut Connection<State>,
        _output: WlOutput,
        _seat: WlSeat,
        tag: &str,
        btn: PointerBtn,
    ) {
        if btn != PointerBtn::Left {
            return;
        }

        let Some(ws) = self
            .groups
            .iter()
            .find_map(|g| g.workspaces.iter().find(|w| w.name.as_deref() == Some(tag)))
        else {
            return;
        };

        ws.workspace_handle.activate(conn);
        self.manager.commit(conn);
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

fn manager_cb(ctx: EventCtx<State, ZextWorkspaceManagerV1>) {
    let ewu = ctx.state.shared_state.get_ewu().unwrap();

    match ctx.event {
        zext_workspace_manager_v1::Event::WorkspaceGroup(group_handle) => {
            ctx.conn.set_callback_for(group_handle, group_cb);
            ewu.groups.push(Group {
                group_handle,
                workspaces: Vec::new(),
                outputs: Vec::new(),
            });
        }
        zext_workspace_manager_v1::Event::Done => ctx.state.tags_updated(ctx.conn, None),
        zext_workspace_manager_v1::Event::Finished => unreachable!(),
    }
}

fn group_cb(ctx: EventCtx<State, ZextWorkspaceGroupHandleV1>) {
    let ewu = ctx.state.shared_state.get_ewu().unwrap();
    let group = ewu
        .groups
        .iter_mut()
        .find(|g| g.group_handle == ctx.proxy)
        .unwrap();

    match ctx.event {
        zext_workspace_group_handle_v1::Event::OutputEnter(output) => {
            group.outputs.push(output);
        }
        zext_workspace_group_handle_v1::Event::OutputLeave(_) => todo!(),
        zext_workspace_group_handle_v1::Event::Workspace(workspace_handle) => {
            ctx.conn.set_callback_for(workspace_handle, workspace_cb);
            group.workspaces.push(Workspace {
                workspace_handle,
                name: None,
                is_focused: false,
                is_urgent: false,
            });
        }
        zext_workspace_group_handle_v1::Event::Remove => {
            todo!();
        }
    }
}

fn workspace_cb(ctx: EventCtx<State, ZextWorkspaceHandleV1>) {
    let ewu = ctx.state.shared_state.get_ewu().unwrap();

    let group = ewu
        .groups
        .iter_mut()
        .find(|g| g.workspaces.iter().any(|w| w.workspace_handle == ctx.proxy))
        .unwrap();
    let workspace = group
        .workspaces
        .iter_mut()
        .find(|w| w.workspace_handle == ctx.proxy)
        .unwrap();

    match ctx.event {
        zext_workspace_handle_v1::Event::Name(name) => {
            workspace.name = Some(name.to_string_lossy().into_owned());
        }
        zext_workspace_handle_v1::Event::Coordinates(_) => (),
        zext_workspace_handle_v1::Event::State(state) => {
            workspace.is_focused = false;
            workspace.is_urgent = false;
            for state in state
                .chunks_exact(4)
                .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
            {
                workspace.is_focused |= state == zext_workspace_handle_v1::State::Active as u32;
                workspace.is_urgent |= state == zext_workspace_handle_v1::State::Urgent as u32;
            }
        }
        zext_workspace_handle_v1::Event::Remove => {
            group.workspaces.retain(|w| w.workspace_handle != ctx.proxy);
            ctx.proxy.destroy(ctx.conn);
        }
    }
}
