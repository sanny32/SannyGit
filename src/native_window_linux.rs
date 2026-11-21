//! Linux window-manager and compositor hints for modal viewports.
use eframe::egui::{Context, Id};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{error::Error, io};
use x11rb::{
    protocol::xproto::{AtomEnum, ConnectionExt, MapState, PropMode, Window},
    wrapper::ConnectionExt as _,
};

fn owner_key() -> Id {
    Id::unique("sannygit.x11_modal_owner")
}

pub fn remember_owner(
    context: &eframe::CreationContext<'_>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let owner = match context.window_handle()?.as_raw() {
        RawWindowHandle::Xlib(handle) => Some(handle.window as Window),
        RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
        RawWindowHandle::Wayland(_) => return wayland::remember_owner(context),
        _ => None,
    };
    context
        .egui_ctx
        .data_mut(|data| data.insert_temp(owner_key(), owner));
    Ok(())
}

pub fn end_modal(context: &Context) {
    wayland::end_modal(context.viewport_id());
}

pub fn shutdown() {
    wayland::shutdown();
}

pub fn begin_modal(context: &Context, title: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    if wayland::has_owner() {
        return wayland::begin_modal(context);
    }
    let owner = context
        .data(|data| data.get_temp::<Option<Window>>(owner_key()).flatten())
        .ok_or_else(|| io::Error::other("Main window handle is unavailable"))?;
    let (connection, _) = x11rb::connect(None)?;
    let root = connection.get_geometry(owner)?.reply()?.root;
    let pid_atom = connection.intern_atom(false, b"_NET_WM_PID")?.reply()?.atom;
    let state_atom = connection
        .intern_atom(false, b"_NET_WM_STATE")?
        .reply()?
        .atom;
    let modal_atom = connection
        .intern_atom(false, b"_NET_WM_STATE_MODAL")?
        .reply()?
        .atom;

    // Immediate viewports start hidden. Find the unmapped dialog on the same
    // screen and in this process before eframe sends its first Visible command.
    let mut candidates = Vec::new();
    for window in connection.query_tree(root)?.reply()?.children {
        if window == owner {
            continue;
        }
        let name = connection
            .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 32)?
            .reply();
        if !name.is_ok_and(|name| name.value == title.as_bytes()) {
            continue;
        }
        let pid = connection
            .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)?
            .reply();
        if pid.is_ok_and(|pid| {
            pid.value32().and_then(|mut values| values.next()) == Some(std::process::id())
        }) && connection
            .get_window_attributes(window)?
            .reply()
            .is_ok_and(|attributes| attributes.map_state == MapState::UNMAPPED)
        {
            candidates.push(window);
        }
    }
    let [window] = candidates.as_slice() else {
        return Err(io::Error::other("Could not identify the native dialog window").into());
    };

    connection
        .change_property32(
            PropMode::REPLACE,
            *window,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[owner],
        )?
        .check()?;
    let state = connection
        .get_property(false, *window, state_atom, AtomEnum::ATOM, 0, 64)?
        .reply()?;
    if !state
        .value32()
        .is_some_and(|mut atoms| atoms.any(|atom| atom == modal_atom))
    {
        connection
            .change_property32(
                PropMode::APPEND,
                *window,
                state_atom,
                AtomEnum::ATOM,
                &[modal_atom],
            )?
            .check()?;
    }
    Ok(())
}

mod wayland {
    use eframe::egui::{Context, ViewportId};
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    use std::{cell::RefCell, error::Error, io};
    use wayland_backend::client::{Backend, ObjectId};
    use wayland_client::{
        delegate_noop,
        globals::{registry_queue_init, BindError, GlobalListContents},
        protocol::wl_registry::WlRegistry,
        Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    };
    use wayland_protocols::xdg::{
        dialog::v1::client::{xdg_dialog_v1::XdgDialogV1, xdg_wm_dialog_v1::XdgWmDialogV1},
        shell::client::xdg_toplevel::XdgToplevel,
    };
    use winit::{platform::wayland::WindowExtWayland, window::Window};

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    struct State {
        connection: Connection,
        queue: EventQueue<Protocol>,
        owner: XdgToplevel,
        manager: Option<XdgWmDialogV1>,
        dialog: Option<(ViewportId, XdgDialogV1)>,
    }

    struct Protocol;

    delegate_noop!(Protocol: ignore XdgWmDialogV1);
    delegate_noop!(Protocol: ignore XdgDialogV1);

    impl Dispatch<WlRegistry, GlobalListContents> for Protocol {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: wayland_client::protocol::wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    pub(super) fn remember_owner(
        context: &eframe::CreationContext<'_>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let window = context
            .winit_window()
            .ok_or_else(|| io::Error::other("Main window is unavailable"))?;
        let RawDisplayHandle::Wayland(display) = window.display_handle()?.as_raw() else {
            return Err(io::Error::other("Wayland display is unavailable").into());
        };
        // SAFETY: winit owns the display for the event loop's lifetime. The
        // foreign backend uses its own queue and does not disconnect it.
        let connection = Connection::from_backend(unsafe {
            Backend::from_foreign_display(display.display.as_ptr().cast())
        });
        let owner = toplevel(&connection, window)?;
        let (globals, queue) = registry_queue_init::<Protocol>(&connection)?;
        let manager = match globals.bind(&queue.handle(), 1..=1, ()) {
            Ok(manager) => Some(manager),
            Err(BindError::NotPresent) => None,
            Err(error) => return Err(error.into()),
        };
        STATE.with_borrow_mut(|state| {
            *state = Some(State {
                connection,
                queue,
                owner,
                manager,
                dialog: None,
            })
        });
        Ok(())
    }

    pub(super) fn has_owner() -> bool {
        STATE.with_borrow(Option::is_some)
    }

    pub(super) fn begin_modal(context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
        let window = eframe::viewport_winit_window(context, context.viewport_id())
            .ok_or_else(|| io::Error::other("Native dialog window is unavailable"))?;
        let viewport = context.viewport_id();
        STATE.with_borrow_mut(|state| {
            let state = state
                .as_mut()
                .ok_or_else(|| io::Error::other("Wayland owner is unavailable"))?;
            if state
                .dialog
                .as_ref()
                .is_some_and(|(active, _)| *active == viewport)
            {
                return Ok(());
            }
            let toplevel = toplevel(&state.connection, &window)?;
            toplevel.set_parent(Some(&state.owner));
            if let Some(manager) = &state.manager {
                let dialog = manager.get_xdg_dialog(&toplevel, &state.queue.handle(), ());
                dialog.set_modal();
                state.dialog = Some((viewport, dialog));
            }
            Ok(())
        })
    }

    pub(super) fn end_modal(viewport: ViewportId) {
        STATE.with_borrow_mut(|state| {
            if let Some(state) = state {
                if state
                    .dialog
                    .as_ref()
                    .is_some_and(|(active, _)| viewport == ViewportId::ROOT || *active == viewport)
                {
                    if let Some((_, dialog)) = state.dialog.take() {
                        dialog.destroy();
                    }
                }
            }
        });
    }

    pub(super) fn shutdown() {
        // Release the foreign backend while winit still owns the display.
        STATE.with_borrow_mut(|state| {
            if let Some(state) = state.take() {
                if let Some((_, dialog)) = state.dialog {
                    dialog.destroy();
                }
                if let Some(manager) = state.manager {
                    manager.destroy();
                }
            }
        });
    }

    fn toplevel(
        connection: &Connection,
        window: &Window,
    ) -> Result<XdgToplevel, Box<dyn Error + Send + Sync>> {
        let proxy = window
            .xdg_toplevel()
            .ok_or_else(|| io::Error::other("Wayland toplevel is unavailable"))?;
        // SAFETY: the supplied live winit window owns this xdg_toplevel on
        // the display wrapped by connection. We only send role requests.
        let id = unsafe { ObjectId::from_ptr(XdgToplevel::interface(), proxy.as_ptr().cast()) }?;
        Ok(XdgToplevel::from_id(connection, id)?)
    }
}
