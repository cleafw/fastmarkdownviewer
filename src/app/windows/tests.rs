use super::*;

#[test]
fn fresh_settings_apply_monokai_to_the_actual_window() {
    let directory = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    let mut app = ViewerApp::restore_from(
        InitialState::Empty,
        &context,
        directory.path().join("new-state.json"),
    );
    frame(&context, &mut app, input(Vec::new()));
    assert_eq!(
        *app.root.theme.lock().unwrap(),
        crate::appearance::ThemeChoice::Monokai
    );
    assert_eq!(
        context.global_style().visuals.panel_fill,
        egui::Color32::from_rgb(39, 40, 34)
    );
    assert_eq!(crate::appearance::text_size_percent(&context), 100);
}

#[test]
fn tab_titles_load_glyphs_without_loading_inactive_documents() {
    // Use a fresh font context for each script: one language must not accidentally
    // load a broad fallback that hides a missing fallback for another language.
    for title in [
        "日本語",
        "中文",
        "한국어",
        "العربية",
        "עברית",
        "हिन्दी",
        "ภาษาไทย",
        "Café Ελληνικά Українська 🙂",
    ] {
        check_tab_title(title);
    }
}

fn check_tab_title(title: &str) {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("English.md");
    let second = directory.path().join(format!("{title}.md"));
    std::fs::write(&first, "# English text only").unwrap();
    std::fs::write(&second, "# Another English document").unwrap();
    let context = egui::Context::default();
    crate::fonts::install(&context);
    let mut app = ViewerApp::new(InitialState::Empty);
    app.root.restore_session(crate::persistence::Window {
        tabs: vec![
            crate::persistence::Tab {
                path: first.clone(),
                scroll: 0.0,
            },
            crate::persistence::Tab {
                path: second,
                scroll: 0.0,
            },
        ],
        active: 0,
        outline: true,
        ..Default::default()
    });
    for _ in 0..4 {
        frame(&context, &mut app, input(Vec::new()));
    }
    assert!(app.root.tabs[1].pending_load);
    assert!(app.root.tabs[1].document.source.is_empty());
    assert_label_glyphs(&context, title);
    // Renaming must also discover the script in a context that has only ever
    // rendered English, independently of the restored-title case above.
    let context = egui::Context::default();
    crate::fonts::install(&context);
    let mut app = ViewerApp::new(InitialState::Path(first));
    for _ in 0..4 {
        frame(&context, &mut app, input(Vec::new()));
    }
    let tab_id = app.root.tabs[0].id;
    app.root
        .rename_tab(&context, tab_id, &format!("renamed {title}.md"))
        .unwrap();
    for _ in 0..4 {
        frame(&context, &mut app, input(Vec::new()));
    }
    assert_label_glyphs(&context, title);
}

#[test]
fn find_and_rename_inputs_load_scripts_absent_from_the_document() {
    for rename in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("English.md");
        std::fs::write(&path, "# English document").unwrap();
        let context = egui::Context::default();
        crate::fonts::install(&context);
        let mut app = ViewerApp::new(InitialState::Path(path));
        let query = "中文 日本語 한국어 हिन्दी ภาษาไทย";
        if rename {
            app.root.rename_dialog = Some(super::super::RenameDialog {
                tab_id: app.root.tabs[0].id,
                name: String::new(),
                focus: true,
                error: None,
            });
        } else {
            app.root.tabs[0].search.open = true;
            app.root.focus_search = true;
        }
        for _ in 0..4 {
            frame(&context, &mut app, input(Vec::new()));
        }
        frame(
            &context,
            &mut app,
            input(vec![egui::Event::Paste(query.into())]),
        );
        for _ in 0..4 {
            frame(&context, &mut app, input(Vec::new()));
        }
        let value = if rename {
            &app.root.rename_dialog.as_ref().unwrap().name
        } else {
            &app.root.tabs[0].search.query
        };
        assert_eq!(value, query);
        assert_label_glyphs(&context, query);
        assert_eq!(app.root.tabs[0].document.source, "# English document");
    }
}

#[test]
fn multilingual_heading_code_and_find_survive_theme_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("English.md");
    let query = "日本語 中文 한국어 हिन्दी ภาษาไทย Café Ελληνικά Українська 🙂";
    std::fs::write(&path, format!("# {query}\n\n`{query}`\n\n{query}\n")).unwrap();
    let context = egui::Context::default();
    crate::fonts::install(&context);
    let mut app = ViewerApp::new(InitialState::Path(path));
    app.root.tabs[0].search.open = true;
    app.root.tabs[0].search.query = query.into();
    for theme in [
        crate::appearance::ThemeChoice::Light,
        crate::appearance::ThemeChoice::Dark,
    ] {
        theme.apply(&context);
        for _ in 0..6 {
            frame(&context, &mut app, input(Vec::new()));
        }
        assert_eq!(app.root.search_status(), Some((1, 3)));
        assert_label_glyphs(&context, query);
        assert!(
            app.root.tabs[0]
                .markdown_cache
                .navigation
                .headings
                .iter()
                .any(|heading| heading.text == query)
        );
    }
}

fn assert_label_glyphs(context: &egui::Context, text: &str) {
    context.fonts_mut(|fonts| {
        let definitions = fonts.definitions();
        for character in text.chars().filter(|character| !character.is_whitespace()) {
            assert!(
                definitions.families[&egui::FontFamily::Proportional]
                    .iter()
                    .any(|name| {
                        let data = &definitions.font_data[name];
                        ttf_parser::Face::parse(&data.font, data.index)
                            .is_ok_and(|face| face.glyph_index(character).is_some())
                    }),
                "no tab-label glyph for {character}; loaded fonts: {:?}",
                definitions.families[&egui::FontFamily::Proportional]
            );
        }
    });
}

#[test]
fn external_open_reuses_tabs_and_revives_a_hidden_root() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("First 日本語.md");
    let second = directory.path().join("Second.markdown");
    std::fs::write(&first, "# First").unwrap();
    std::fs::write(&second, "# Second").unwrap();
    let context = egui::Context::default();
    let mut app = ViewerApp::new(InitialState::Empty);
    app.open_external_files(&[first.clone(), second.clone(), first.clone()], &context);
    assert_eq!(app.tab_count(), 2);
    assert_eq!(
        app.document_path(),
        Some(first.canonicalize().unwrap().as_path())
    );
    app.open_external_files(&[directory.path().join("missing.md")], &context);
    assert_eq!(app.tab_count(), 2);
    assert!(app.error_message().is_some());
    app.root_closed = true;
    app.root.close_requested = true;
    app.root.tabs.clear();
    app.open_external_files(&[second], &context);
    assert!(!app.root_closed);
    assert!(!app.root.close_requested);
    assert_eq!(app.tab_count(), 1);
}

#[test]
fn saved_session_restores_lazily_and_keeps_reading_position() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.md");
    let second = directory.path().join("second.md");
    std::fs::write(&first, "# First\n\nparagraph\n\n".repeat(200)).unwrap();
    std::fs::write(&second, "# Second").unwrap();
    let path = directory.path().join("state.json");
    let context = egui::Context::default();
    {
        let mut app =
            ViewerApp::restore_from(InitialState::Path(first.clone()), &context, path.clone());
        app.root.load(&second);
        app.root.active = 0;
        app.root.tabs[0].scroll_offset = 500.0;
        app.root.show_outline = false;
        app.root.hide_tabs = true;
        app.root.hide_open_files = true;
        *app.root.theme.lock().unwrap() = crate::appearance::ThemeChoice::Nord;
        context.set_zoom_factor(1.25);
        crate::appearance::set_text_size_percent(&context, 150);
        frame(&context, &mut app, input(Vec::new()));
        app.root.tabs[0].scroll_offset = 500.0;
        crate::network::set_automatic_images(&context, false);
        context.data_mut(|data| data.insert_temp(egui::Id::new("word_wrap"), false));
        crate::appearance::DocumentWidth::FitWindow.apply(&context);
    }
    let context = egui::Context::default();
    let mut restored = ViewerApp::restore_from(InitialState::Empty, &context, path.clone());
    assert_eq!(restored.tab_count(), 2);
    assert!(
        restored
            .root
            .tabs
            .iter()
            .all(|tab| tab.pending_load && tab.document.source.is_empty())
    );
    assert_eq!(
        *restored.root.theme.lock().unwrap(),
        crate::appearance::ThemeChoice::Nord
    );
    assert!(!crate::network::automatic_images(&context));
    assert!(!restored.root.show_outline);
    assert!(restored.root.hide_tabs);
    assert!(restored.root.hide_open_files);
    assert_eq!(
        crate::appearance::DocumentWidth::current(&context),
        crate::appearance::DocumentWidth::FitWindow
    );
    let mut sizing = input(Vec::new());
    sizing.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(30_000.0, 30_000.0),
    ));
    frame(&context, &mut restored, sizing);
    for _ in 0..12 {
        frame(&context, &mut restored, input(Vec::new()));
    }
    assert!((context.zoom_factor() - 1.25).abs() < 0.001);
    assert_eq!(crate::appearance::text_size_percent(&context), 150);
    assert!((restored.document_scroll_offset() - 500.0).abs() < 1.0);
    assert!(restored.root.tabs[1].pending_load);
    // A file changed while its restored tab was inactive: opening it reads current content.
    std::fs::write(&second, "# Updated").unwrap();
    restored.root.active = 1;
    frame(&context, &mut restored, input(Vec::new()));
    assert_eq!(restored.root.tabs[1].document.source, "# Updated");
    for tick in 0..12 {
        let mut raw = input(Vec::new());
        raw.time = Some(f64::from(tick + 1));
        let output = frame(&context, &mut restored, raw);
        if tick == 11 {
            assert!(
                output.viewport_output[&egui::ViewportId::ROOT].repaint_delay
                    > std::time::Duration::from_secs(1)
            );
        }
    }
    let explicit =
        ViewerApp::restore_from(InitialState::Path(first), &egui::Context::default(), path);
    assert_eq!(explicit.tab_count(), 1);
}

#[test]
fn missing_restored_file_can_be_reloaded_and_closed_tabs_stay_closed() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("missing.md");
    let path = directory.path().join("state.json");
    std::fs::write(&file, "# Document").unwrap();
    let context = egui::Context::default();
    drop(ViewerApp::restore_from(
        InitialState::Path(file.clone()),
        &context,
        path.clone(),
    ));
    std::fs::remove_file(&file).unwrap();
    let mut app = ViewerApp::restore_from(InitialState::Empty, &context, path.clone());
    frame(&context, &mut app, input(Vec::new()));
    assert!(app.root.tabs[0].load_error.is_some());
    std::fs::write(&file, "# Back again").unwrap();
    app.root.reload(&context);
    assert!(app.root.tabs[0].load_error.is_none());
    assert_eq!(app.root.tabs[0].document.source, "# Back again");
    app.root.close_tab(0, &context);
    app.root.close_requested = true;
    app.handle_root_close(&context);
    drop(app);
    assert_eq!(
        ViewerApp::restore_from(InitialState::Empty, &context, path).tab_count(),
        0
    );
}

#[test]
fn quit_restores_all_windows_and_last_detached_close_preserves_its_tabs() {
    let (directory, context, mut app, start) = fixture();
    let path = directory.path().join("state.json");
    app.persistence = Some((path.clone(), context.clone()));
    drag(&context, &mut app, start, egui::pos2(900.0, 150.0));
    context.data_mut(|data| data.insert_temp(egui::Id::new("quit_application"), true));
    app.handle_root_close(&context);
    drop(app);
    let mut app = ViewerApp::restore_from(InitialState::Empty, &context, path.clone());
    assert_eq!(app.detached.len(), 1);
    assert_eq!(app.root.tabs.len(), 0);
    app.root.close_requested = true;
    app.handle_root_close(&context);
    let id = *app.detached.keys().next().unwrap();
    app.pending.lock().unwrap().push(WindowEvent::Close(id));
    app.process_events();
    app.handle_root_close(&context);
    drop(app);
    let app = ViewerApp::restore_from(InitialState::Empty, &context, path);
    assert_eq!(app.root.tabs.len(), 1);
    assert_eq!(app.detached.len(), 0);
}

fn input(events: Vec<egui::Event>) -> egui::RawInput {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    // A monitor to the left of the primary display, with native title bar/borders.
    viewport.inner_rect = Some(egui::Rect::from_min_size(
        egui::pos2(-1200.0, 80.0),
        egui::vec2(800.0, 600.0),
    ));
    viewport.outer_rect = Some(egui::Rect::from_min_max(
        egui::pos2(-1208.0, 50.0),
        egui::pos2(-392.0, 688.0),
    ));
    input
}

fn frame(context: &egui::Context, app: &mut ViewerApp, input: egui::RawInput) -> egui::FullOutput {
    let mut output = context.run_ui(input, |ui| app.show(ui));
    output.textures_delta.clear();
    output
}

#[test]
fn idle_windows_stop_requesting_repaints() {
    let (_directory, context, mut app, _center) = fixture();
    for empty in [false, true] {
        if empty {
            app.root.tabs.clear();
        }
        let mut output = None;
        for tick in 0..12 {
            let mut raw = input(Vec::new());
            raw.time = Some(f64::from(tick + if empty { 12 } else { 0 }));
            output = Some(frame(&context, &mut app, raw));
        }
        let output = output.unwrap();
        let viewport = &output.viewport_output[&egui::ViewportId::ROOT];
        assert!(
            viewport.repaint_delay > std::time::Duration::from_secs(1),
            "An idle window must let the event loop sleep: {:?}",
            viewport.repaint_delay
        );
    }
}

fn assert_native_title(output: &egui::FullOutput, id: egui::ViewportId, expected: &str) {
    assert!(output.viewport_output[&id].commands.iter().any(
        |command| matches!(command, egui::ViewportCommand::Title(title) if title == expected)
    ));
}

#[test]
fn native_title_tracks_open_switch_rename_and_last_tab_close() {
    let (directory, context, mut app, _) = fixture();
    let second = directory.path().join("second.md");
    std::fs::write(&second, "# Second").unwrap();
    app.root.load(&second);
    assert_native_title(
        &frame(&context, &mut app, input(Vec::new())),
        egui::ViewportId::ROOT,
        "second.md — FastMarkdownViewer",
    );
    app.root.active = 0;
    assert_native_title(
        &frame(&context, &mut app, input(Vec::new())),
        egui::ViewportId::ROOT,
        "tear-out.md — FastMarkdownViewer",
    );
    let id = app.root.tabs[0].id;
    app.root.rename_tab(&context, id, "renamed.md").unwrap();
    assert_native_title(
        &frame(&context, &mut app, input(Vec::new())),
        egui::ViewportId::ROOT,
        "renamed.md — FastMarkdownViewer",
    );
    while !app.root.tabs.is_empty() {
        app.root.close_tab(0, &context);
    }
    assert_native_title(
        &frame(&context, &mut app, input(Vec::new())),
        egui::ViewportId::ROOT,
        "FastMarkdownViewer",
    );
}

#[test]
fn detached_window_title_updates_then_allows_sleep() {
    let (_directory, context, mut app, start) = fixture();
    crate::fonts::install(&context);
    let output = drag(&context, &mut app, start, egui::pos2(900.0, 150.0));
    let id = *app.detached.first_key_value().unwrap().0;
    let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
    child_frame(&context, id, &*callback, Vec::new(), false);
    {
        let mut window = app.detached[&id].state.lock().unwrap();
        let tab_id = window.tabs[0].id;
        window
            .rename_tab(&context, tab_id, "日本語 中文 한국어.md")
            .unwrap();
    }
    assert_native_title(
        &child_frame(&context, id, &*callback, Vec::new(), false),
        id,
        "日本語 中文 한국어.md — FastMarkdownViewer",
    );
    for _ in 0..30 {
        child_frame(&context, id, &*callback, Vec::new(), false);
    }
    let output = child_frame(&context, id, &*callback, Vec::new(), false);
    assert_label_glyphs(&context, "日本語 中文 한국어");
    assert!(output.viewport_output[&id].repaint_delay > std::time::Duration::from_secs(1));
}

fn button(position: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[allow(clippy::cast_possible_truncation)]
fn tab_center(output: &egui::FullOutput, title: &str) -> egui::Pos2 {
    let rect = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == egui::accesskit::Role::Button && node.label() == Some(title)
        })
        .filter_map(|(_, node)| node.bounds())
        .min_by(|a, b| a.y0.total_cmp(&b.y0))
        .unwrap();
    egui::pos2(
        f64::midpoint(rect.x0, rect.x1) as f32,
        f64::midpoint(rect.y0, rect.y1) as f32,
    )
}

fn fixture() -> (tempfile::TempDir, egui::Context, ViewerApp, egui::Pos2) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tear-out.md");
    std::fs::write(
        &path,
        format!("# Heading\n\n{}", "marker paragraph\n\n".repeat(100)),
    )
    .unwrap();
    let context = egui::Context::default();
    context.set_embed_viewports(false);
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Path(path));
    let output = frame(&context, &mut app, input(Vec::new()));
    let center = tab_center(&output, "tear-out.md");
    (directory, context, app, center)
}

fn drag(
    context: &egui::Context,
    app: &mut ViewerApp,
    start: egui::Pos2,
    end: egui::Pos2,
) -> egui::FullOutput {
    frame(
        context,
        app,
        input(vec![egui::Event::PointerMoved(start), button(start, true)]),
    );
    frame(context, app, input(vec![egui::Event::PointerMoved(end)]));
    frame(context, app, input(vec![button(end, false)]))
}

fn child_frame(
    context: &egui::Context,
    id: egui::ViewportId,
    callback: &egui::DeferredViewportUiCallback,
    events: Vec<egui::Event>,
    close: bool,
) -> egui::FullOutput {
    let mut raw = input(events);
    raw.viewport_id = id;
    let mut info = raw.viewports[&egui::ViewportId::ROOT].clone();
    info.parent = Some(egui::ViewportId::ROOT);
    if close {
        info.events.push(egui::ViewportEvent::Close);
    }
    raw.viewports.insert(id, info);
    let mut output = context.run_ui(raw, |ui| callback(ui));
    output.textures_delta.clear();
    output
}

#[test]
fn outside_release_moves_document_and_reading_state_without_reloading() {
    let (_directory, context, mut app, start) = fixture();
    app.root.tabs[0].search.open = true;
    app.root.tabs[0].search.query = "marker".into();
    frame(&context, &mut app, input(Vec::new()));
    for _ in 0..30 {
        frame(&context, &mut app, input(Vec::new()));
    }
    app.root.tabs[0].scroll_offset = 250.0;
    app.root.show_outline = false;
    let original_source = app.root.tabs[0].document.source.as_ptr();
    let outside = egui::pos2(-80.0, 150.0);
    let output = drag(&context, &mut app, start, outside);
    assert_eq!(app.tab_count(), 0);
    assert_eq!(app.detached_window_count(), 1);
    let (&id, child) = app.detached.first_key_value().unwrap();
    assert!(output.viewport_output[&id].class == egui::ViewportClass::Deferred);
    assert_eq!(child.builder.position, Some(egui::pos2(-1360.0, 210.0)));
    let state = child.state.lock().unwrap();
    assert_eq!(state.tabs[0].document.source.as_ptr(), original_source);
    assert_eq!(state.search_status(), Some((1, 100)));
    assert!((state.tabs[0].scroll_offset - 250.0).abs() < 1.0);
    assert!(!state.show_outline);
    assert!(Arc::ptr_eq(&state.tab_ids, &app.root.tab_ids));
    assert!(Arc::ptr_eq(&state.theme, &app.root.theme));
}

#[test]
fn internal_releases_and_escape_cancel_without_losing_the_tab() {
    let (_directory, context, mut app, start) = fixture();
    // Releasing over content or native title bar is still inside the original window.
    drag(&context, &mut app, start, egui::pos2(400.0, 180.0));
    drag(&context, &mut app, start, egui::pos2(400.0, -15.0));
    assert_eq!(app.detached_window_count(), 0);
    let outside = egui::pos2(900.0, 200.0);
    frame(
        &context,
        &mut app,
        input(vec![egui::Event::PointerMoved(start), button(start, true)]),
    );
    frame(
        &context,
        &mut app,
        input(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]),
    );
    frame(
        &context,
        &mut app,
        input(vec![
            egui::Event::PointerMoved(outside),
            button(outside, false),
        ]),
    );
    assert_eq!(app.tab_count(), 1);
    assert_eq!(app.detached_window_count(), 0);
    // Cancellation must not disable later drags.
    drag(&context, &mut app, start, outside);
    assert_eq!(app.detached_window_count(), 1);
}

#[test]
fn closing_original_keeps_child_alive_and_last_window_close_exits() {
    let (_directory, context, mut app, start) = fixture();
    let output = drag(&context, &mut app, start, egui::pos2(900.0, 150.0));
    let id = *app.detached.first_key_value().unwrap().0;
    let callback = output.viewport_output[&id].viewport_ui_cb.clone().unwrap();
    let mut close_root = input(Vec::new());
    close_root
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let output = frame(&context, &mut app, close_root);
    assert!(app.root_closed);
    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(commands.contains(&egui::ViewportCommand::CancelClose));
    assert!(commands.contains(&egui::ViewportCommand::Visible(false)));
    child_frame(&context, id, &*callback, Vec::new(), false);
    assert_eq!(app.detached[&id].state.lock().unwrap().tab_count(), 1);
    child_frame(&context, id, &*callback, Vec::new(), true);
    let output = frame(&context, &mut app, input(Vec::new()));
    assert_eq!(app.detached_window_count(), 0);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close)
    );
}

#[test]
fn detached_windows_can_tear_out_again_with_unique_ids() {
    let (_directory, context, mut app, start) = fixture();
    let output = drag(&context, &mut app, start, egui::pos2(900.0, 150.0));
    let first = *app.detached.first_key_value().unwrap().0;
    let callback = output.viewport_output[&first]
        .viewport_ui_cb
        .clone()
        .unwrap();
    let child = child_frame(&context, first, &*callback, Vec::new(), false);
    let center = tab_center(&child, "tear-out.md");
    child_frame(
        &context,
        first,
        &*callback,
        vec![egui::Event::PointerMoved(center), button(center, true)],
        false,
    );
    let outside = egui::pos2(900.0, 180.0);
    child_frame(
        &context,
        first,
        &*callback,
        vec![egui::Event::PointerMoved(outside)],
        false,
    );
    child_frame(
        &context,
        first,
        &*callback,
        vec![button(outside, false)],
        false,
    );
    frame(&context, &mut app, input(Vec::new()));
    assert_eq!(app.detached_window_count(), 2);
    assert_eq!(app.detached[&first].state.lock().unwrap().tab_count(), 0);
    let second = app.detached.keys().find(|id| **id != first).unwrap();
    assert_eq!(app.detached[second].state.lock().unwrap().tab_count(), 1);
}

#[test]
fn dragging_tabs_reorders_without_changing_active_document_or_tear_out() {
    let (directory, context, mut app, _) = fixture();
    let second = directory.path().join("second.md");
    std::fs::write(&second, "# Second").unwrap();
    app.root.load(&second);
    let output = frame(&context, &mut app, input(Vec::new()));
    let first = tab_center(&output, "tear-out.md");
    let second_center = tab_center(&output, "second.md");
    let active_id = app.root.tabs[app.root.active].id;
    drag(&context, &mut app, first, second_center);
    assert_eq!(app.root.tabs[0].document.title, "second.md");
    assert_eq!(app.root.tabs[1].document.title, "tear-out.md");
    assert_eq!(app.root.tabs[app.root.active].id, active_id);
    assert_eq!(app.detached_window_count(), 0);
    let output = frame(&context, &mut app, input(Vec::new()));
    let moved = tab_center(&output, "tear-out.md");
    drag(&context, &mut app, moved, egui::pos2(920.0, 150.0));
    assert_eq!(app.detached_window_count(), 1);
    assert_eq!(app.root.tabs[0].document.title, "second.md");
}

#[test]
fn reload_keeps_tab_and_reading_state_and_failed_reload_keeps_document() {
    let (directory, context, mut app, _) = fixture();
    let path = directory.path().join("tear-out.md");
    frame(
        &context,
        &mut app,
        input(vec![egui::Event::Key {
            key: egui::Key::PageDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]),
    );
    let scroll = app.document_scroll_offset();
    let id = app.root.tabs[0].id;
    std::fs::write(
        &path,
        format!("# Updated\n\n{}", "New paragraph.\n\n".repeat(120)),
    )
    .unwrap();
    for (key, modifiers) in [
        (egui::Key::F5, egui::Modifiers::NONE),
        (egui::Key::R, egui::Modifiers::COMMAND),
    ] {
        frame(
            &context,
            &mut app,
            input(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }]),
        );
        assert!(app.root.tabs[0].document.source.contains("Updated"));
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.root.tabs[0].id, id);
        assert!((app.document_scroll_offset() - scroll).abs() < 2.0);
    }
    std::fs::write(&path, [0xff]).unwrap();
    app.root.reload(&context);
    assert!(app.error_message().unwrap().contains("Could not reload"));
    assert!(app.root.tabs[0].document.source.contains("Updated"));
}

#[test]
fn markdown_links_reuse_tabs_and_navigate_heading_slugs() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("README.md");
    let target = dir.path().join("other.md");
    std::fs::write(&source, "# Home\n").unwrap();
    std::fs::write(
        &target,
        format!(
            "# Same!\n\n{}\n# Same!\n\n{}\n# Tail {{#custom}}\n\n{}",
            "paragraph\n\n".repeat(70),
            "middle\n\n".repeat(70),
            "end\n\n".repeat(30)
        ),
    )
    .unwrap();
    let context = egui::Context::default();
    let mut app = ViewerApp::new(InitialState::Path(source));
    app.root.handle_link("other.md#same-1");
    assert_eq!(app.root.tabs.len(), 2);
    for _ in 0..90 {
        frame(&context, &mut app, input(vec![]));
    }
    assert!(app.root.document_scroll_offset() > 500.0);
    let second_position = app.root.document_scroll_offset();
    app.root.handle_link("#custom");
    for _ in 0..90 {
        frame(&context, &mut app, input(vec![]));
    }
    assert!(app.root.document_scroll_offset() > second_position);
    app.root.handle_link("#same");
    for _ in 0..90 {
        frame(&context, &mut app, input(vec![]));
    }
    assert!(
        app.root.document_scroll_offset() < 100.0,
        "offset {} headings {:?}",
        app.root.document_scroll_offset(),
        app.root.tabs[app.root.active]
            .markdown_cache
            .navigation
            .headings
    );
    app.root.handle_link("./other.md#missing");
    assert_eq!(app.root.tabs.len(), 2);
    frame(&context, &mut app, input(vec![]));
    assert!(
        app.root
            .error_message()
            .unwrap()
            .contains("Heading not found")
    );
    app.root.handle_link("README.md");
    assert_eq!(app.root.active, 0);
    assert_eq!(app.root.tabs.len(), 2);
    app.root.handle_link("missing.md#same");
    assert_eq!(app.root.active, 0);
    assert!(app.root.error_message().unwrap().contains("missing.md"));
}

#[test]
fn rename_preserves_tab_order_active_tab_and_reading_state() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("old.md");
    let second = dir.path().join("second.md");
    std::fs::write(&source, "# Original\n").unwrap();
    std::fs::write(&second, "# Second\n").unwrap();
    let context = egui::Context::default();
    let mut app = ViewerApp::new(InitialState::Path(source.clone()));
    app.root.load(&second);
    let tab_id = app.root.tabs[0].id;
    app.root.tabs[0].scroll_offset = 123.0;
    app.root.tabs[0].search.query = "Original".into();
    app.root
        .rename_tab(&context, tab_id, "new.markdown")
        .unwrap();
    assert_eq!(app.root.active, 1);
    assert_eq!(app.root.tabs[0].id, tab_id);
    assert_eq!(
        app.root.tabs[0].scroll_offset.to_bits(),
        123.0_f32.to_bits()
    );
    assert_eq!(app.root.tabs[0].search.query, "Original");
    assert_eq!(app.root.tabs[0].document.title, "new.markdown");
    assert!(!source.exists());
    app.root.load(&dir.path().join("new.markdown"));
    assert_eq!(app.root.tabs.len(), 2);
    assert_eq!(app.root.active, 0);
    assert!(app.root.rename_tab(&context, tab_id, "second.md").is_err());
    assert_eq!(app.root.tabs[0].document.title, "new.markdown");
}

#[test]
fn quick_filter_switches_files_and_focus_mode_preserves_layout() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.md");
    let second = directory.path().join("second.md");
    std::fs::write(&first, "# First").unwrap();
    std::fs::write(&second, "# Second").unwrap();
    let context = egui::Context::default();
    let mut app = ViewerApp::new(InitialState::Path(first.clone()));
    app.root.load(&second);
    app.root.hide_tabs = true;
    app.root.hide_open_files = true;

    let press = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        &context,
        &mut app,
        input(vec![press(egui::Key::F9, egui::Modifiers::NONE)]),
    );
    assert!(app.root.focus_mode);
    frame(
        &context,
        &mut app,
        input(vec![press(egui::Key::F9, egui::Modifiers::NONE)]),
    );
    assert!(!app.root.focus_mode);
    assert!(app.root.hide_tabs && app.root.hide_open_files && app.root.show_outline);

    frame(
        &context,
        &mut app,
        input(vec![press(egui::Key::P, egui::Modifiers::COMMAND)]),
    );
    assert!(!app.root.hide_open_files);
    assert!(context.memory(|m| m.has_focus(ViewerWindow::file_filter_id(&context))));
    frame(
        &context,
        &mut app,
        input(vec![egui::Event::Text("FIRST".to_owned())]),
    );
    frame(
        &context,
        &mut app,
        input(vec![press(egui::Key::Enter, egui::Modifiers::NONE)]),
    );
    assert_eq!(
        app.document_path(),
        Some(first.canonicalize().unwrap().as_path())
    );
    frame(
        &context,
        &mut app,
        input(vec![press(egui::Key::P, egui::Modifiers::COMMAND)]),
    );
    frame(
        &context,
        &mut app,
        input(vec![egui::Event::Text("no match".into())]),
    );
    frame(
        &context,
        &mut app,
        input(vec![press(egui::Key::Enter, egui::Modifiers::NONE)]),
    );
    assert_eq!(
        app.document_path(),
        Some(first.canonicalize().unwrap().as_path())
    );
    frame(
        &context,
        &mut app,
        input(vec![press(egui::Key::Escape, egui::Modifiers::NONE)]),
    );
    assert!(app.root.file_filter.is_empty());
    assert!(!context.memory(|m| m.has_focus(ViewerWindow::file_filter_id(&context))));
    assert_eq!(app.tab_count(), 2);
}

#[test]
fn saved_language_overrides_system_and_is_persisted() {
    use crate::language::Language;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("language.json");
    let context = egui::Context::default();
    let app = ViewerApp::restore_from(InitialState::Empty, &context, path.clone());
    assert!(Language::current(&context) == Language::System);
    Language::Chinese.apply(&context);
    drop(app);

    let context = egui::Context::default();
    let _app = ViewerApp::restore_from(InitialState::Empty, &context, path);
    assert_eq!(Language::current(&context).text("File", "文件"), "文件");
}
