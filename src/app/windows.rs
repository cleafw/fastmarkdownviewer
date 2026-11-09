//! Native window ownership. Only this host creates/removes deferred viewports.
use super::{AppServices, InitialState, PlatformServices, ViewerWindow};
use eframe::egui;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};

struct DetachedWindow {
    state: Arc<Mutex<ViewerWindow>>,
    builder: egui::ViewportBuilder,
}

struct Spawn {
    window: ViewerWindow,
    position: Option<egui::Pos2>,
    size: egui::Vec2,
}

enum WindowEvent {
    Spawn(Box<Spawn>),
    Close(egui::ViewportId),
}

/// One process with independently repainted native document windows.
pub struct ViewerApp {
    #[cfg(target_os = "macos")]
    native_events: Option<fmv_macos_events::Inbox>,
    persistence: Option<(std::path::PathBuf, egui::Context)>,
    exit_session: Option<Vec<crate::persistence::Window>>,
    root: ViewerWindow,
    detached: BTreeMap<egui::ViewportId, DetachedWindow>,
    pending: Arc<Mutex<Vec<WindowEvent>>>,
    next_window_id: u64,
    root_closed: bool,
}

impl ViewerApp {
    #[must_use]
    pub fn new(initial: InitialState) -> Self {
        Self::with_services(initial, Arc::new(PlatformServices))
    }

    #[must_use]
    pub fn with_services(initial: InitialState, services: Arc<dyn AppServices>) -> Self {
        Self {
            #[cfg(target_os = "macos")]
            native_events: None,
            persistence: None,
            exit_session: None,
            root: ViewerWindow::with_services(initial, services),
            detached: BTreeMap::new(),
            pending: Arc::default(),
            next_window_id: 0,
            root_closed: false,
        }
    }

    /// Restore local preferences; reopen the last session only for a bare launch.
    #[must_use]
    pub fn with_saved_state(initial: InitialState, context: &egui::Context) -> Self {
        let Some(path) = crate::persistence::path() else {
            crate::appearance::ThemeChoice::default().apply(context);
            crate::language::Language::default().apply(context);
            return Self::new(initial);
        };
        Self::restore_from(initial, context, path)
    }

    fn restore_from(
        initial: InitialState,
        context: &egui::Context,
        path: std::path::PathBuf,
    ) -> Self {
        let state = crate::persistence::read(&path);
        let restore_session = matches!(initial, InitialState::Empty);
        let mut app = Self::new(initial);
        *app.root.theme.lock().expect("appearance lock") = state.theme;
        state.theme.apply(context);
        context.set_zoom_factor(state.zoom);
        crate::appearance::set_text_size_percent(context, state.text_size_percent);
        crate::fonts::restore(context, &state.fonts);
        state.language.apply(context);
        crate::network::set_automatic_images(context, state.automatic_images);
        context.data_mut(|data| data.insert_temp(egui::Id::new("word_wrap"), state.word_wrap));
        state.document_width.apply(context);
        if restore_session {
            let mut windows = state.windows.into_iter();
            if let Some(window) = windows.next() {
                app.root.restore_session(window);
            }
            for saved in windows.filter(|window| !window.tabs.is_empty()) {
                let mut window =
                    ViewerWindow::with_services(InitialState::Empty, app.root.services.clone());
                window.theme = app.root.theme.clone();
                window.tab_ids = app.root.tab_ids.clone();
                window.restore_session(saved);
                app.spawn(Spawn {
                    window,
                    position: None,
                    size: egui::vec2(900.0, 700.0),
                });
            }
        }
        app.persistence = Some((path, context.clone()));
        app
    }

    fn session(&self) -> Vec<crate::persistence::Window> {
        let mut windows = Vec::new();
        if !self.root_closed {
            windows.push(self.root.session());
        }
        windows.extend(
            self.detached
                .values()
                .map(|window| window.state.lock().expect("document window lock").session()),
        );
        windows
    }

    /// Connect Finder open/quit events to the existing document and exit paths.
    #[cfg(target_os = "macos")]
    #[must_use]
    pub fn with_native_events(
        mut self,
        inbox: fmv_macos_events::Inbox,
        context: &egui::Context,
    ) -> Self {
        fmv_macos_events::install_edit_menu();
        let wake_context = context.clone();
        inbox.set_wake(move || wake_context.request_repaint_of(egui::ViewportId::ROOT));
        self.native_events = Some(inbox);
        self.process_native_events(context);
        self
    }

    #[cfg(target_os = "macos")]
    fn process_native_events(&mut self, context: &egui::Context) {
        let events = self
            .native_events
            .as_ref()
            .map(fmv_macos_events::Inbox::drain)
            .unwrap_or_default();
        for event in events {
            match event {
                fmv_macos_events::Event::Open(paths) => self.open_external_files(&paths, context),
                fmv_macos_events::Event::Quit => {
                    context
                        .data_mut(|data| data.insert_temp(egui::Id::new("quit_application"), true));
                }
            }
        }
    }

    #[cfg(any(target_os = "macos", test))]
    fn open_external_files(&mut self, paths: &[std::path::PathBuf], context: &egui::Context) {
        for path in paths {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            let existing = self.detached.iter().find_map(|(&id, window)| {
                let mut window = window.state.lock().expect("document window lock");
                let index = window
                    .tabs
                    .iter()
                    .position(|tab| tab.document.path == canonical)?;
                window.active = index;
                Some(id)
            });
            if let Some(id) = existing {
                context.send_viewport_cmd_to(id, egui::ViewportCommand::Minimized(false));
                context.send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
            } else {
                self.root_closed = false;
                self.root.close_requested = false;
                self.root.load(path);
                context.send_viewport_cmd_to(
                    egui::ViewportId::ROOT,
                    egui::ViewportCommand::Visible(true),
                );
                context.send_viewport_cmd_to(
                    egui::ViewportId::ROOT,
                    egui::ViewportCommand::Minimized(false),
                );
                context.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
            }
        }
    }

    #[must_use]
    pub fn document_path(&self) -> Option<&Path> {
        self.root.document_path()
    }
    #[must_use]
    pub fn error_message(&self) -> Option<&str> {
        self.root.error_message()
    }
    #[must_use]
    pub fn document_scroll_offset(&self) -> f32 {
        self.root.document_scroll_offset()
    }
    #[must_use]
    pub fn tab_count(&self) -> usize {
        self.root.tab_count()
    }
    #[must_use]
    pub const fn outline_visible(&self) -> bool {
        self.root.outline_visible()
    }
    #[must_use]
    pub fn search_status(&self) -> Option<(usize, usize)> {
        self.root.search_status()
    }
    #[must_use]
    pub fn detached_window_count(&self) -> usize {
        self.detached.len()
    }

    fn spawn(&mut self, spawn: Spawn) {
        let id = egui::ViewportId::from_hash_of(("document_window", self.next_window_id));
        self.next_window_id += 1;
        let mut builder = egui::ViewportBuilder::default()
            .with_drag_and_drop(true)
            .with_title(format!(
                "{} — FastMarkdownViewer",
                spawn.window.tabs[0].document.title
            ))
            .with_app_id("FastMarkdownViewer")
            .with_inner_size(spawn.size)
            .with_min_inner_size([420.0, 280.0])
            .with_icon(crate::platform::app_icon());
        if let Some(position) = spawn.position {
            builder = builder.with_position(position);
        }
        self.detached.insert(
            id,
            DetachedWindow {
                state: Arc::new(Mutex::new(spawn.window)),
                builder,
            },
        );
    }

    fn take_spawn(window: &mut ViewerWindow) -> Option<Spawn> {
        let request = window.detach_request.take()?;
        let index = window
            .tabs
            .iter()
            .position(|tab| tab.id == request.tab_id)?;
        let mut tab = window.remove_tab(index);
        // Geometry belongs to the old viewport; reading position, query and caches move intact.
        tab.markdown_cache.navigation.clear();
        tab.markdown_cache.navigation.scroll_target = None;
        window.tab_drag = None;
        let child = ViewerWindow {
            tabs: vec![tab],
            active: 0,
            tab_ids: window.tab_ids.clone(),
            error: None,
            services: window.services.clone(),
            theme: window.theme.clone(),
            show_outline: window.show_outline,
            hide_open_files: window.hide_open_files,
            hide_tabs: window.hide_tabs,
            focus_mode: false,
            file_filter: String::new(),
            focus_file_filter: false,
            focus_search: false,
            tab_drag: None,
            drag_cancelled: false,
            detach_request: None,
            close_requested: false,
            rename_dialog: None,
            native_title: None,
        };
        Some(Spawn {
            window: child,
            position: request.position,
            size: request.size,
        })
    }

    fn process_events(&mut self) {
        let events = std::mem::take(&mut *self.pending.lock().expect("window events lock"));
        for event in events {
            match event {
                WindowEvent::Spawn(spawn) => self.spawn(*spawn),
                WindowEvent::Close(id) => {
                    if self.root_closed && self.detached.len() == 1 {
                        self.exit_session = Some(self.session());
                    }
                    self.detached.remove(&id);
                }
            }
        }
    }

    fn handle_root_close(&mut self, context: &egui::Context) {
        if context
            .data_mut(|data| data.remove_temp::<bool>(egui::Id::new("quit_application")))
            .unwrap_or(false)
        {
            self.exit_session = Some(self.session());
            self.root_closed = true;
            self.detached.clear();
            context.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let close =
            self.root.close_requested || context.input(|input| input.viewport().close_requested());
        if close && !self.root_closed {
            if self.detached.is_empty() {
                self.exit_session = Some(self.session());
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                // egui's root must exist until the process exits. Hide it while independent
                // document windows remain, and release its documents immediately.
                self.root_closed = true;
                for tab in &self.root.tabs {
                    for uri in tab.image_uris.lock().expect("image references lock").iter() {
                        context.forget_image(uri);
                    }
                }
                self.root.tabs.clear();
                context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                context.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
        if self.root_closed && self.detached.is_empty() {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Render the root and register independently repainted document windows.
    ///
    /// # Panics
    /// Panics if a window-state mutex was poisoned by an earlier panic.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let context = ui.ctx().clone();
        self.process_events();
        if !self.root_closed {
            self.root.show(ui);

            if let Some(spawn) = Self::take_spawn(&mut self.root) {
                self.spawn(spawn);
                context.request_repaint();
            }
        }

        self.handle_root_close(&context);

        for (&id, window) in &self.detached {
            let state = window.state.clone();
            let pending = self.pending.clone();
            context.show_viewport_deferred(id, window.builder.clone(), move |ui, _class| {
                let mut window = state.lock().expect("document window lock");
                let native_close = ui.input(|input| input.viewport().close_requested());
                if !native_close && !window.close_requested {
                    window.show(ui);
                }
                if native_close || window.close_requested {
                    for tab in &window.tabs {
                        for uri in tab.image_uris.lock().expect("image references lock").iter() {
                            ui.ctx().forget_image(uri);
                        }
                    }
                    window.close_requested = true;
                    pending
                        .lock()
                        .expect("window events lock")
                        .push(WindowEvent::Close(id));
                    ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
                } else if let Some(spawn) = Self::take_spawn(&mut window) {
                    pending
                        .lock()
                        .expect("window events lock")
                        .push(WindowEvent::Spawn(Box::new(spawn)));
                    ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
                }
            });
        }
    }
}

impl Drop for ViewerApp {
    fn drop(&mut self) {
        let Some((path, context)) = &self.persistence else {
            return;
        };
        let state = crate::persistence::State {
            theme: *self.root.theme.lock().expect("appearance lock"),
            fonts: crate::fonts::preferences(context),
            language: crate::language::Language::current(context),
            document_width: crate::appearance::DocumentWidth::current(context),
            zoom: context.zoom_factor(),
            text_size_percent: crate::appearance::text_size_percent(context),
            automatic_images: crate::network::automatic_images(context),
            word_wrap: context.data(|data| {
                data.get_temp::<bool>(egui::Id::new("word_wrap"))
                    .unwrap_or(true)
            }),
            windows: self.exit_session.take().unwrap_or_else(|| self.session()),
            ..crate::persistence::State::default()
        };
        if let Err(error) = crate::persistence::write(path, &state) {
            eprintln!("Could not save FastMarkdownViewer settings: {error}");
        }
    }
}

impl eframe::App for ViewerApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "macos")]
        self.process_native_events(context);
        // eframe can skip UI when every window is minimized/occluded. Lifetime
        // events must still be processed so the final close always exits.
        self.process_events();
        self.handle_root_close(context);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;
