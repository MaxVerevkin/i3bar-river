use wayrs_client::connection::Connection;
use wayrs_client::global::*;
use wayrs_client::object::ObjectId;
use wayrs_client::proxy::Proxy;

use super::*;
use crate::protocol::*;
use crate::state::State;

pub struct ExtWorkspaceUnstable {
    groups: Vec<Group>,
    known_outputs: Vec<WlOutput>,
    callback: WmInfoCallback,
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
    pub fn bind(
        conn: &mut Connection<State>,
        globals: &Globals,
        callback: WmInfoCallback,
    ) -> Option<Self> {
        let _: ZextWorkspaceManagerV1 = globals.bind_with_cb(conn, 1..=1, manager_cb).ok()?;
        Some(Self {
            groups: Vec::new(),
            known_outputs: Vec::new(),
            callback,
        })
    }
}

impl ExtWorkspaceUnstable {
    pub fn new_ouput(&mut self, _: &mut Connection<State>, output: WlOutput) {
        self.known_outputs.push(output);
    }

    pub fn output_removed(&mut self, _: &mut Connection<State>, output: WlOutput) {
        self.known_outputs.retain(|&o| o != output);
        for group in &mut self.groups {
            group.outputs.retain(|&id| id != output.id());
        }
    }

    pub fn click_on_tag(
        &mut self,
        _: &mut Connection<State>,
        _: WlOutput,
        _: WlSeat,
        _: &str,
        _: PointerBtn,
    ) {
    }
}

fn manager_cb(
    conn: &mut Connection<State>,
    s: &mut State,
    _: ZextWorkspaceManagerV1,
    event: zext_workspace_manager_v1::Event,
) {
    let WmInfoProvider::EWU(state) = &mut s.shared_state.wm_info_provider else { unreachable!() };

    match event {
        zext_workspace_manager_v1::Event::WorkspaceGroup(group_handle) => {
            conn.set_callback_for(group_handle, group_cb);
            state.groups.push(Group {
                group_handle,
                workspaces: Vec::new(),
                outputs: Vec::new(),
            });
        }
        zext_workspace_manager_v1::Event::Done => {
            let mut events = Vec::new();
            for &output in &state.known_outputs {
                if let Some(group) = state
                    .groups
                    .iter()
                    .find(|g| g.outputs.contains(&output.id()))
                {
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
                    events.push((output, tags));
                }
            }

            let cb = state.callback;
            for (output, tags) in events {
                let info = WmInfo {
                    layout_name: None,
                    tags,
                };
                (cb)(conn, s, output, info);
            }
        }
        zext_workspace_manager_v1::Event::Finished => unreachable!(),
    }
}

fn group_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    group_handle: ZextWorkspaceGroupHandleV1,
    event: zext_workspace_group_handle_v1::Event,
) {
    let WmInfoProvider::EWU(state) = &mut state.shared_state.wm_info_provider else { unreachable!() };
    let group = state
        .groups
        .iter_mut()
        .find(|g| g.group_handle == group_handle)
        .unwrap();

    match event {
        zext_workspace_group_handle_v1::Event::OutputEnter(output) => {
            group.outputs.push(output);
        }
        zext_workspace_group_handle_v1::Event::OutputLeave(_) => todo!(),
        zext_workspace_group_handle_v1::Event::Workspace(workspace_handle) => {
            conn.set_callback_for(workspace_handle, workspace_cb);
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

fn workspace_cb(
    conn: &mut Connection<State>,
    state: &mut State,
    workspace_handle: ZextWorkspaceHandleV1,
    event: zext_workspace_handle_v1::Event,
) {
    let WmInfoProvider::EWU(state) = &mut state.shared_state.wm_info_provider else { unreachable!() };
    let group = state
        .groups
        .iter_mut()
        .find(|g| {
            g.workspaces
                .iter()
                .any(|w| w.workspace_handle == workspace_handle)
        })
        .unwrap();
    let workspace = group
        .workspaces
        .iter_mut()
        .find(|w| w.workspace_handle == workspace_handle)
        .unwrap();

    match event {
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
            group
                .workspaces
                .retain(|w| w.workspace_handle != workspace_handle);
            workspace_handle.destroy(conn);
        }
    }
}
