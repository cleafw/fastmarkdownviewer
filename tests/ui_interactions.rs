use std::{
    io,
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use eframe::egui;
use fast_markdown_viewer::{
    app::{AppServices, InitialState, ViewerApp},
    fonts,
};
use url::Url;

const VIEWPORT_WIDTH: f32 = 720.0;
const VIEWPORT_HEIGHT: f32 = 240.0;

#[derive(Debug)]
struct TestDrop(PathBuf);

impl egui::DroppedFile for TestDrop {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|error| error.to_string())
    }
}

#[derive(Debug, Default)]
struct TestServices {
    selected_file: Mutex<Option<PathBuf>>,
    browser_urls: Mutex<Vec<Url>>,
    revealed_paths: Mutex<Vec<PathBuf>>,
}

impl AppServices for TestServices {
    fn choose_markdown_file(&self) -> Option<PathBuf> {
        self.selected_file.lock().unwrap().take()
    }

    fn open_browser(&self, url: &Url) -> io::Result<()> {
        self.browser_urls.lock().unwrap().push(url.clone());
        Ok(())
    }

    fn reveal_in_file_manager(&self, path: &Path) -> io::Result<()> {
        self.revealed_paths.lock().unwrap().push(path.to_owned());
        Ok(())
    }
}

// These interaction tests locate controls by their English accessibility labels.
fn english_context() -> egui::Context {
    let context = egui::Context::default();
    fast_markdown_viewer::language::Language::English.apply(&context);
    context
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("feature-matrix.md")
}

fn input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(VIEWPORT_WIDTH, VIEWPORT_HEIGHT),
        )),
        events,
        ..Default::default()
    }
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn run_frame(
    context: &egui::Context,
    app: &mut ViewerApp,
    raw_input: egui::RawInput,
) -> egui::FullOutput {
    let mut output = context.run_ui(raw_input, |ui| app.show(ui));
    output.textures_delta.clear();
    output
}

fn accesskit_update(output: &egui::FullOutput) -> &egui::accesskit::TreeUpdate {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accessibility tree was not produced")
}

fn node_with_text(
    update: &egui::accesskit::TreeUpdate,
    role: egui::accesskit::Role,
    expected: &str,
) -> egui::accesskit::NodeId {
    update
        .nodes
        .iter()
        .find_map(|(id, node)| {
            if node.role() != role {
                return None;
            }
            let own_text = node.label().or_else(|| node.value()).unwrap_or_default();
            let child_text = node
                .children()
                .iter()
                .filter_map(|child_id| {
                    update
                        .nodes
                        .iter()
                        .find(|(candidate_id, _)| candidate_id == child_id)
                        .and_then(|(_, child)| child.value().or_else(|| child.label()))
                })
                .collect::<String>();
            (own_text.contains(expected) || child_text.contains(expected)).then_some(*id)
        })
        .unwrap_or_else(|| {
            let summary = update
                .nodes
                .iter()
                .map(|(id, node)| (id, node.role(), node.label(), node.value(), node.children()))
                .collect::<Vec<_>>();
            panic!("no {role:?} node contained {expected:?}; tree: {summary:#?}")
        })
}

fn accesskit_action(
    action: egui::accesskit::Action,
    target_node: egui::accesskit::NodeId,
    data: Option<egui::accesskit::ActionData>,
) -> egui::Event {
    egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
        action,
        target_tree: egui::accesskit::TreeId::ROOT,
        target_node,
        data,
    })
}

#[test]
fn empty_window_paints_the_whole_viewport_and_exposes_main_menus() {
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Empty);
    let output = run_frame(&context, &mut app, input(Vec::new()));
    let viewport = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(VIEWPORT_WIDTH, VIEWPORT_HEIGHT),
    );

    assert!(output.shapes.iter().any(|clipped| {
        matches!(
            &clipped.shape,
            egui::epaint::Shape::Rect(rect)
                if rect.fill != egui::Color32::TRANSPARENT
                    && rect.rect.contains(viewport.min)
                    && rect.rect.contains(viewport.max - egui::vec2(0.1, 0.1))
        )
    }));

    let update = accesskit_update(&output);
    for expected in ["File", "Settings", "Open Markdown file"] {
        assert!(
            update.nodes.iter().any(|(_, node)| {
                node.label()
                    .or_else(|| node.value())
                    .is_some_and(|text| text.contains(expected))
            }),
            "accessibility tree did not contain {expected:?}"
        );
    }
}

#[test]
fn control_wheel_zooms_in_and_out() {
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(fixture()));
    run_frame(&context, &mut app, input(Vec::new()));
    let initial_zoom = context.zoom_factor();
    let modifiers = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;

    run_frame(
        &context,
        &mut app,
        input(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 120.0),
            modifiers,
            phase: egui::TouchPhase::Move,
        }]),
    );
    assert!(app.document_scroll_offset().abs() < f32::EPSILON);
    run_frame(&context, &mut app, input(Vec::new()));
    let zoomed_in = context.zoom_factor();
    assert!(zoomed_in > initial_zoom);

    run_frame(
        &context,
        &mut app,
        input(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            modifiers,
            phase: egui::TouchPhase::Move,
        }]),
    );
    run_frame(&context, &mut app, input(Vec::new()));
    assert!(context.zoom_factor() < zoomed_in);
}

#[test]
fn find_and_heading_shortcuts_toggle_and_release_keyboard_focus() {
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(fixture()));
    run_frame(&context, &mut app, input(Vec::new()));
    for expected in [true, false, true, false] {
        run_frame(
            &context,
            &mut app,
            input(vec![key(egui::Key::F, egui::Modifiers::COMMAND)]),
        );
        assert_eq!(app.search_status().is_some(), expected);
    }
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::PageDown, egui::Modifiers::NONE)]),
    );
    assert!(app.document_scroll_offset() > 0.0);
    for expected in [false, true] {
        run_frame(
            &context,
            &mut app,
            input(vec![key(egui::Key::H, egui::Modifiers::COMMAND)]),
        );
        assert_eq!(app.outline_visible(), expected);
    }
}

#[test]
fn empty_window_explains_find_and_heading_shortcuts() {
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Empty);
    let mut raw = input(Vec::new());
    raw.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(900.0, 700.0),
    ));
    let output = run_frame(&context, &mut app, raw);
    let text = accesskit_update(&output)
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().or_else(|| node.value()))
        .collect::<String>();
    for expected in [
        "Ctrl+F",
        "Ctrl+H",
        "Show / hide Find",
        "Show / hide headings",
        "Drag a tab outside",
    ] {
        assert!(
            text.contains(fast_markdown_viewer::platform::shortcut_label(expected).as_ref()),
            "missing shortcut help: {expected}"
        );
    }
}

#[test]
fn default_fonts_cover_common_unicode_emoji() {
    let context = english_context();
    fonts::install(&context);
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Path(fixture()));
    let output = run_frame(&context, &mut app, input(Vec::new()));
    let rendered_text = accesskit_update(&output)
        .nodes
        .iter()
        .filter_map(|(_, node)| node.value().or_else(|| node.label()))
        .collect::<String>();
    assert!(rendered_text.contains("😀 🎉 ✅ ❤️"));

    let font = egui::FontId::proportional(16.0);
    for character in ['😀', '🎉', '✅', '❤'] {
        assert!(has_actual_glyph(&context, &font, character));
    }
}

#[test]
fn mouse_wheel_scrolls_the_real_document_surface_without_panicking() {
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(fixture()));
    run_frame(&context, &mut app, input(Vec::new()));

    for _ in 0..6 {
        run_frame(
            &context,
            &mut app,
            input(vec![
                egui::Event::PointerMoved(egui::pos2(300.0, 120.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -180.0),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                },
            ]),
        );
    }

    assert!(
        app.document_scroll_offset() > 0.0,
        "wheel input did not move the document viewport"
    );
}

#[test]
fn page_space_home_end_and_arrow_keys_scroll_the_document() {
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(fixture()));
    run_frame(&context, &mut app, input(Vec::new()));

    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::PageDown, egui::Modifiers::NONE)]),
    );
    let page_offset = app.document_scroll_offset();
    assert!(page_offset > 0.0, "Page Down did not move the viewport");

    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)]),
    );
    let arrow_offset = app.document_scroll_offset();
    assert!(arrow_offset > page_offset);

    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::PageUp, egui::Modifiers::NONE)]),
    );
    let page_up_offset = app.document_scroll_offset();
    assert!(page_up_offset < arrow_offset, "Page Up did not move upward");

    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Space, egui::Modifiers::NONE)]),
    );
    assert!(
        app.document_scroll_offset() > page_up_offset,
        "Space did not move downward"
    );

    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::End, egui::Modifiers::NONE)]),
    );
    assert!(app.document_scroll_offset() > page_offset);

    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Home, egui::Modifiers::NONE)]),
    );
    assert!(app.document_scroll_offset().abs() < f32::EPSILON);
}

#[test]
fn dropping_files_opens_tabs_and_deduplicates_paths() {
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Empty);
    let path = fixture();
    let mut drop_input = input(Vec::new());
    drop_input.dropped_files = vec![Arc::new(TestDrop(path.clone()))];
    run_frame(&context, &mut app, drop_input);

    assert_eq!(
        app.document_path(),
        Some(path.canonicalize().unwrap().as_path())
    );
    assert!(app.error_message().is_none());

    let mut multiple_input = input(Vec::new());
    multiple_input.dropped_files = vec![Arc::new(TestDrop(path.clone())), Arc::new(TestDrop(path))];
    run_frame(&context, &mut app, multiple_input);
    assert!(app.error_message().is_none());
    assert_eq!(app.tab_count(), 1);
}

#[test]
fn open_files_sidebar_switches_documents_with_top_tabs_hidden() {
    use egui::accesskit::{Action, Role};

    let directory = tempfile::tempdir().unwrap();
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Empty);
    let paths = (0..40)
        .map(|index| {
            let path = directory.path().join(format!("file-{index:02}.md"));
            std::fs::write(&path, "# Example document").unwrap();
            path
        })
        .collect::<Vec<_>>();
    let mut dropped = input(Vec::new());
    dropped.dropped_files = paths
        .iter()
        .cloned()
        .map(|path| Arc::new(TestDrop(path)) as egui::DroppedFileHandle)
        .collect();
    run_frame(&context, &mut app, dropped);

    for (role, label) in [(Role::Button, "View"), (Role::CheckBox, "Top tab bar")] {
        let output = run_frame(&context, &mut app, input(Vec::new()));
        let id = node_with_text(accesskit_update(&output), role, label);
        run_frame(
            &context,
            &mut app,
            input(vec![accesskit_action(Action::Click, id, None)]),
        );
    }
    let output = run_frame(&context, &mut app, input(Vec::new()));
    let tree = accesskit_update(&output);
    assert_eq!(
        tree.nodes
            .iter()
            .filter(|(_, node)| node.label() == Some("file-00.md"))
            .count(),
        1,
        "the first file should appear only in the sidebar",
    );
    let id = node_with_text(tree, Role::Button, "file-00.md");
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(Action::Click, id, None)]),
    );
    assert_eq!(
        app.document_path(),
        Some(paths[0].canonicalize().unwrap().as_path())
    );
    assert_eq!(app.tab_count(), paths.len());
}

#[test]
fn control_o_uses_the_picker_and_loads_the_selected_file() {
    let context = english_context();
    let services = Arc::new(TestServices::default());
    let path = fixture();
    *services.selected_file.lock().unwrap() = Some(path.clone());
    let mut app = ViewerApp::with_services(InitialState::Empty, services);

    let windows_command = egui::Modifiers::CTRL | egui::Modifiers::COMMAND;
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::O, windows_command)]),
    );

    assert_eq!(
        app.document_path(),
        Some(path.canonicalize().unwrap().as_path())
    );
    assert!(app.error_message().is_none());
}

#[test]
fn invalid_drop_is_a_visible_error_instead_of_a_crash() {
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Empty);
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("definitely-missing.md");
    let mut drop_input = input(Vec::new());
    drop_input.dropped_files = vec![Arc::new(TestDrop(missing))];
    run_frame(&context, &mut app, drop_input);

    assert!(app.document_path().is_none());
    assert!(
        app.error_message()
            .is_some_and(|error| error.contains("Cannot open"))
    );

    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]),
    );
    assert!(app.error_message().is_none());
}

#[test]
fn accessibility_clicks_dispatch_safe_links_and_leave_unsafe_links_inert() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        "[Browser test](https://example.com/test) · [Chapter test](chapter.md) · [Unsafe test](javascript:alert(1))"
    )
    .unwrap();

    let context = english_context();
    context.enable_accesskit();
    let services = Arc::new(TestServices::default());
    let mut app =
        ViewerApp::with_services(InitialState::Path(file.path().to_owned()), services.clone());
    let output = run_frame(&context, &mut app, input(Vec::new()));
    let update = accesskit_update(&output);
    let browser = node_with_text(update, egui::accesskit::Role::Label, "Browser test");
    let chapter = node_with_text(update, egui::accesskit::Role::Label, "Chapter test");
    let unsafe_link = node_with_text(update, egui::accesskit::Role::Label, "Unsafe test");

    for node in [browser, chapter, unsafe_link] {
        run_frame(
            &context,
            &mut app,
            input(vec![accesskit_action(
                egui::accesskit::Action::Click,
                node,
                None,
            )]),
        );
    }

    assert_eq!(
        services.browser_urls.lock().unwrap().as_slice(),
        &[Url::parse("https://example.com/test").unwrap()]
    );
    assert!(services.revealed_paths.lock().unwrap().is_empty());
    assert!(app.error_message().unwrap().contains("chapter.md"));
}

#[test]
#[allow(clippy::cast_possible_truncation)]
fn accessible_text_selection_can_be_copied() {
    const SENTENCE: &str = "Selectable regression sentence.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(file, "{SENTENCE}").unwrap();

    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Path(file.path().to_owned()));
    let output = run_frame(&context, &mut app, input(Vec::new()));
    let update = accesskit_update(&output);
    let (run_id, run) = update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == egui::accesskit::Role::TextRun
                && node.value().is_some_and(|value| value.contains(SENTENCE))
        })
        .expect("selectable text run was not exposed to accessibility");
    let parent_id = update
        .nodes
        .iter()
        .find(|(_, node)| node.children().contains(run_id))
        .map(|(id, _)| *id)
        .expect("text run had no selectable parent");
    let run_length = run.character_lengths().len();
    let bounds = run.bounds().expect("text run had no screen bounds");
    let click_position = egui::pos2(
        f64::midpoint(bounds.x0, bounds.x1) as f32,
        f64::midpoint(bounds.y0, bounds.y1) as f32,
    );
    run_frame(
        &context,
        &mut app,
        input(vec![
            egui::Event::PointerMoved(click_position),
            egui::Event::PointerButton {
                pos: click_position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: click_position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]),
    );
    let selection = egui::accesskit::TextSelection {
        anchor: egui::accesskit::TextPosition {
            node: *run_id,
            character_index: 0,
        },
        focus: egui::accesskit::TextPosition {
            node: *run_id,
            character_index: run_length,
        },
    };

    let selected = run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::SetTextSelection,
            parent_id,
            Some(egui::accesskit::ActionData::SetTextSelection(selection)),
        )]),
    );
    let selected_update = accesskit_update(&selected);
    let selected_node = selected_update
        .nodes
        .iter()
        .find(|(id, _)| *id == parent_id)
        .map(|(_, node)| node)
        .expect("selected text parent was not updated");
    assert!(
        selected_node.text_selection().is_some(),
        "accessibility selection was not applied"
    );
    let copied = run_frame(&context, &mut app, input(vec![egui::Event::Copy]));
    assert!(
        copied.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text.contains(SENTENCE))
        }),
        "copy output was {:?}",
        copied.platform_output.commands
    );
}

#[test]
fn system_theme_and_common_windows_scale_factors_render_without_panicking() {
    for (scale, theme) in [
        (1.0, egui::Theme::Light),
        (1.5, egui::Theme::Dark),
        (2.0, egui::Theme::Light),
    ] {
        let context = english_context();
        context.set_theme(egui::ThemePreference::System);
        let mut app = ViewerApp::new(InitialState::Path(fixture()));
        let mut raw_input = input(Vec::new());
        raw_input.system_theme = Some(theme);
        raw_input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(scale);
        run_frame(&context, &mut app, raw_input);
        assert_eq!(context.theme(), theme);
    }
}

#[test]
fn find_searches_rendered_text_and_code_without_stealing_typing_keys() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(
        file,
        "# Search fixture\n\nHello **world**\n\n{}\n\n```rust\n// hello world\n```\n",
        "Filler paragraph.\n\n".repeat(70)
    )
    .unwrap();
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(file.path().to_owned()));
    run_frame(&context, &mut app, input(Vec::new()));
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::F, egui::Modifiers::COMMAND)]),
    );
    run_frame(
        &context,
        &mut app,
        input(vec![egui::Event::Text("hello*world".into())]),
    );
    assert_eq!(app.search_status(), Some((1, 2)));
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]),
    );
    for _ in 0..20 {
        run_frame(&context, &mut app, input(Vec::new()));
    }
    assert_eq!(app.search_status(), Some((2, 2)));
    assert!(app.document_scroll_offset() > 500.0);
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Enter, egui::Modifiers::SHIFT)]),
    );
    assert_eq!(app.search_status(), Some((1, 2)));
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]),
    );
    assert_eq!(app.search_status(), None);
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Home, egui::Modifiers::NONE)]),
    );
    assert!(app.document_scroll_offset() < 1.0);
}

#[test]
fn tabs_preserve_scroll_and_bad_opens_preserve_documents() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.md");
    let second = directory.path().join("second.md");
    std::fs::write(&first, "Paragraph.\n\n".repeat(100)).unwrap();
    std::fs::write(&second, "# Second").unwrap();
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(first.clone()));
    run_frame(&context, &mut app, input(Vec::new()));
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::PageDown, egui::Modifiers::NONE)]),
    );
    let scroll = app.document_scroll_offset();
    let mut dropped = input(Vec::new());
    dropped.dropped_files = vec![
        Arc::new(TestDrop(second.clone())),
        Arc::new(TestDrop(directory.path().join("missing.md"))),
    ];
    run_frame(&context, &mut app, dropped);
    assert_eq!(app.tab_count(), 2);
    assert_eq!(
        app.document_path(),
        Some(second.canonicalize().unwrap().as_path())
    );
    assert!(app.error_message().is_some());
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Tab, egui::Modifiers::CTRL)]),
    );
    assert_eq!(
        app.document_path(),
        Some(first.canonicalize().unwrap().as_path())
    );
    assert!((app.document_scroll_offset() - scroll).abs() < 1.0);
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::W, egui::Modifiers::COMMAND)]),
    );
    assert_eq!(app.tab_count(), 1);
    assert_eq!(
        app.document_path(),
        Some(second.canonicalize().unwrap().as_path())
    );
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::W, egui::Modifiers::COMMAND)]),
    );
    assert_eq!(app.tab_count(), 0);
    assert!(app.document_path().is_none());
}

#[test]
fn outline_navigates_duplicate_and_setext_headings_and_can_be_hidden() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(
        file,
        "# Duplicate\n\n{}\n\nDuplicate\n=========\n",
        "Paragraph.\n\n".repeat(80)
    )
    .unwrap();
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Path(file.path().to_owned()));
    run_frame(&context, &mut app, input(Vec::new()));
    let output = run_frame(&context, &mut app, input(Vec::new()));
    let mut headings: Vec<_> = accesskit_update(&output)
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == egui::accesskit::Role::Button && node.label() == Some("Duplicate")
        })
        .map(|(id, node)| (*id, node.bounds().unwrap().y0))
        .collect();
    headings.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert_eq!(headings.len(), 2);
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::Click,
            headings[1].0,
            None,
        )]),
    );
    for _ in 0..20 {
        run_frame(&context, &mut app, input(Vec::new()));
    }
    assert!(app.document_scroll_offset() > 500.0);
    run_frame(
        &context,
        &mut app,
        input(vec![key(
            egui::Key::O,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        )]),
    );
    assert!(!app.outline_visible());
}

#[test]
#[cfg(target_os = "windows")]
fn windows_fallbacks_contain_actual_multilingual_glyphs_in_text_and_code() {
    const SAMPLE: &str = "English 日本語 中文 한국어 العربية עברית 🙂 हिन्दी देवनागरी ภาษาไทย";
    let context = english_context();
    fonts::install(&context);
    fonts::ensure_for_text(&context, SAMPLE);
    let mut output = context.run_ui(input(Vec::new()), |ui| {
        ui.label(SAMPLE);
    });
    output.textures_delta.clear();
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let font = egui::FontId::new(16.0, family);
        for character in SAMPLE
            .chars()
            .filter(|character| !character.is_whitespace())
        {
            assert!(
                has_actual_glyph(&context, &font, character),
                "Missing glyph: {character}"
            );
        }
    }
}

fn has_actual_glyph(context: &egui::Context, font: &egui::FontId, character: char) -> bool {
    // egui 0.36's has_glyph compares face identity with the replacement face,
    // giving false negatives for valid characters in that same fallback face.
    context.fonts_mut(|fonts| {
        let definitions = fonts.definitions();
        definitions.families[&font.family].iter().any(|name| {
            let data = &definitions.font_data[name];
            ttf_parser::Face::parse(&data.font, data.index)
                .is_ok_and(|face| face.glyph_index(character).is_some())
        })
    })
}

#[test]
fn find_reveals_matches_beyond_a_code_blocks_horizontal_viewport() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(
        file,
        "```text\n{}far_away_match\n```",
        "padding ".repeat(60)
    )
    .unwrap();
    let context = english_context();
    context.data_mut(|data| data.insert_temp(egui::Id::new("word_wrap"), false));
    let mut app = ViewerApp::new(InitialState::Path(file.path().to_owned()));
    run_frame(&context, &mut app, input(Vec::new()));
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::F, egui::Modifiers::COMMAND)]),
    );
    run_frame(
        &context,
        &mut app,
        input(vec![egui::Event::Text("far_away_match".into())]),
    );
    for _ in 0..30 {
        run_frame(&context, &mut app, input(Vec::new()));
    }
    let output = run_frame(&context, &mut app, input(Vec::new()));
    let highlight = egui::Color32::from_rgba_unmultiplied(255, 140, 0, 100);
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == highlight && shape.clip_rect.contains_rect(rect.rect))
    }), "active code match was not revealed inside the horizontal clip");
}

#[test]
fn remote_images_default_on_can_be_blocked_and_loaded_individually() {
    use fast_markdown_viewer::network;
    #[derive(Default)]
    struct TestImages(std::sync::atomic::AtomicUsize);
    impl egui::load::BytesLoader for TestImages {
        fn id(&self) -> &'static str {
            "test remote image bytes"
        }
        fn load(&self, _: &egui::Context, uri: &str) -> egui::load::BytesLoadResult {
            if !uri.starts_with("https://") {
                return Err(egui::load::LoadError::NotSupported);
            }
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(egui::load::BytesPoll::Ready { size: None, bytes: egui::load::Bytes::Static(br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="red"/></svg>"#), mime: Some("image/svg+xml".into()) })
        }
        fn forget(&self, _: &str) {}
        fn forget_all(&self) {}
        fn byte_size(&self) -> usize {
            0
        }
    }
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        file,
        "![One](https://example.com/one.svg)\n\n![Two](https://example.com/two.svg)"
    )
    .unwrap();
    let context = english_context();
    context.enable_accesskit();
    network::install(&context);
    let images = Arc::new(TestImages::default());
    context.add_bytes_loader(images.clone());
    let mut app = ViewerApp::new(InitialState::Path(file.path().to_owned()));
    assert!(network::automatic_images(&context));
    network::set_automatic_images(&context, false);
    let output = run_frame(&context, &mut app, input(Vec::new()));
    assert_eq!(images.0.load(std::sync::atomic::Ordering::Relaxed), 0);
    let button = node_with_text(
        accesskit_update(&output),
        egui::accesskit::Role::Button,
        "Load image",
    );
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::Click,
            button,
            None,
        )]),
    );
    for _ in 0..20 {
        run_frame(&context, &mut app, input(Vec::new()));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(images.0.load(std::sync::atomic::Ordering::Relaxed), 1);
    let output = run_frame(&context, &mut app, input(Vec::new()));
    assert_eq!(
        accesskit_update(&output)
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::accesskit::Role::Button
                && node.label() == Some("Load image"))
            .count(),
        1
    );
    network::set_automatic_images(&context, true);
    for _ in 0..20 {
        run_frame(&context, &mut app, input(Vec::new()));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(images.0.load(std::sync::atomic::Ordering::Relaxed), 2);
    network::set_automatic_images(&context, false);
    let output = run_frame(&context, &mut app, input(Vec::new()));
    assert_eq!(
        accesskit_update(&output)
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::accesskit::Role::Button
                && node.label() == Some("Load image"))
            .count(),
        2
    );
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::W, egui::Modifiers::COMMAND)]),
    );
    assert_eq!(
        context
            .loaders()
            .texture
            .lock()
            .iter()
            .map(|loader| loader.byte_size())
            .sum::<usize>(),
        0
    );
}

#[test]
fn clicking_heading_and_cross_document_links_navigates_in_window() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("README.md");
    let other = dir.path().join("other.md");
    std::fs::write(
        &source,
        format!(
            "[Jump](#section)\n\n[Other](other.md#target)\n\n{}\n# Section\n\n{}",
            "body\n\n".repeat(40),
            "tail\n\n".repeat(20)
        ),
    )
    .unwrap();
    std::fs::write(
        &other,
        format!(
            "# Other\n\n{}\n# Target\n\n{}",
            "body\n\n".repeat(40),
            "tail\n\n".repeat(20)
        ),
    )
    .unwrap();
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Path(source));
    let output = run_frame(&context, &mut app, input(vec![]));
    let jump = node_with_text(
        accesskit_update(&output),
        egui::accesskit::Role::Label,
        "Jump",
    );
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::Click,
            jump,
            None,
        )]),
    );
    for _ in 0..90 {
        run_frame(&context, &mut app, input(vec![]));
    }
    assert!(app.document_scroll_offset() > 500.0);
    run_frame(
        &context,
        &mut app,
        input(vec![key(egui::Key::Home, egui::Modifiers::NONE)]),
    );
    let output = run_frame(&context, &mut app, input(vec![]));
    let other_link = node_with_text(
        accesskit_update(&output),
        egui::accesskit::Role::Label,
        "Other",
    );
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::Click,
            other_link,
            None,
        )]),
    );
    for _ in 0..90 {
        run_frame(&context, &mut app, input(vec![]));
    }
    assert_eq!(app.tab_count(), 2);
    assert_eq!(
        app.document_path(),
        Some(other.canonicalize().unwrap().as_path())
    );
    assert!(app.document_scroll_offset() > 500.0);
}

#[test]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::too_many_lines)] // Exercises the complete context-menu/dialog transaction.
fn tab_menu_reveals_and_renames_the_real_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("old.md");
    std::fs::write(&source, "# Original").unwrap();
    let context = english_context();
    context.enable_accesskit();
    let services = Arc::new(TestServices::default());
    let mut app = ViewerApp::with_services(InitialState::Path(source.clone()), services.clone());
    for action in [
        if cfg!(target_os = "windows") {
            "Show in Explorer"
        } else {
            "Show in file manager"
        },
        "Rename file…",
    ] {
        let output = run_frame(&context, &mut app, input(vec![]));
        let update = accesskit_update(&output);
        let bounds = update
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.role() == egui::accesskit::Role::Button && node.label() == Some("old.md")
            })
            .filter_map(|(_, node)| node.bounds())
            .min_by(|a, b| a.y0.total_cmp(&b.y0))
            .unwrap();
        let pos = egui::pos2(
            bounds.x0.midpoint(bounds.x1) as f32,
            bounds.y0.midpoint(bounds.y1) as f32,
        );
        for pressed in [true, false] {
            run_frame(
                &context,
                &mut app,
                input(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]),
            );
        }
        let output = run_frame(&context, &mut app, input(vec![]));
        let item = node_with_text(
            accesskit_update(&output),
            egui::accesskit::Role::Button,
            action,
        );
        run_frame(
            &context,
            &mut app,
            input(vec![accesskit_action(
                egui::accesskit::Action::Click,
                item,
                None,
            )]),
        );
    }
    assert_eq!(
        services.revealed_paths.lock().unwrap().as_slice(),
        &[source.canonicalize().unwrap()]
    );
    run_frame(&context, &mut app, input(vec![]));
    run_frame(
        &context,
        &mut app,
        input(vec![
            key(egui::Key::A, egui::Modifiers::COMMAND),
            egui::Event::Text("renamed".into()),
        ]),
    );
    let output = run_frame(&context, &mut app, input(vec![]));
    let rename = node_with_text(
        accesskit_update(&output),
        egui::accesskit::Role::Button,
        "Rename",
    );
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::Click,
            rename,
            None,
        )]),
    );
    assert!(!source.exists());
    assert_eq!(
        app.document_path(),
        Some(
            dir.path()
                .join("renamed.md")
                .canonicalize()
                .unwrap()
                .as_path()
        )
    );
    assert_eq!(app.tab_count(), 1);
}

#[test]
fn document_reflows_to_ultrawide_viewport_and_back() {
    use fast_markdown_viewer::appearance::DocumentWidth;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    let paragraph = "This paragraph should use the available document width. ".repeat(100);
    writeln!(file, "{paragraph}").unwrap();
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(file.path().to_owned()));
    let mut sizes = Vec::new();
    for (width, setting) in [
        (900.0, DocumentWidth::FitWindow),
        (3440.0, DocumentWidth::FitWindow),
        (900.0, DocumentWidth::FitWindow),
        (3440.0, DocumentWidth::Comfortable),
    ] {
        setting.apply(&context);
        let mut output = egui::FullOutput::default();
        for _ in 0..8 {
            let mut raw = input(vec![]);
            raw.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 1400.0),
            ));
            output = run_frame(&context, &mut app, raw);
        }
        let size = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().starts_with("This paragraph") => {
                    Some(text.galley.size())
                }
                _ => None,
            })
            .expect("paragraph rendered");
        assert!(size.x < width, "prose must wrap inside viewport");
        sizes.push(size);
    }
    assert!(
        sizes[1].x > 2800.0,
        "ultrawide prose remained capped: {sizes:?}"
    );
    assert!(
        sizes[1].y < sizes[0].y / 2.0,
        "paragraph did not reflow: {sizes:?}"
    );
    assert!((sizes[2].x - sizes[0].x).abs() < 1.0);
    assert!((sizes[2].y - sizes[0].y).abs() < 1.0);
    assert!(sizes[3].x <= 960.0);
    assert!(sizes[3].y > sizes[1].y * 2.0);
}

#[test]
fn settings_width_is_independent_of_word_wrap() {
    use fast_markdown_viewer::appearance::DocumentWidth;
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Empty);
    assert_eq!(DocumentWidth::current(&context), DocumentWidth::Comfortable);
    for (role, label) in [
        (egui::accesskit::Role::Button, "Settings"),
        (egui::accesskit::Role::Button, "Document width:"),
        (egui::accesskit::Role::RadioButton, "Fit window"),
    ] {
        let output = run_frame(&context, &mut app, input(vec![]));
        let node = node_with_text(accesskit_update(&output), role, label);
        run_frame(
            &context,
            &mut app,
            input(vec![accesskit_action(
                egui::accesskit::Action::Click,
                node,
                None,
            )]),
        );
    }
    assert_eq!(DocumentWidth::current(&context), DocumentWidth::FitWindow);
    assert!(context.data(|data| {
        data.get_temp::<bool>(egui::Id::new("word_wrap"))
            .unwrap_or(true)
    }));
    // Disabling wrapping must not silently change the chosen column width.
    context.data_mut(|data| data.insert_temp(egui::Id::new("word_wrap"), false));
    run_frame(&context, &mut app, input(vec![]));
    assert_eq!(DocumentWidth::current(&context), DocumentWidth::FitWindow);
}

#[test]
fn document_overflow_can_scroll_horizontally() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    // Deep indentation is an object that cannot fit even when its text wraps.
    write!(file, "Document marker\n\n{}Overflow\n", "> ".repeat(60)).unwrap();
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(file.path().to_owned()));
    let marker_x = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Document marker" => {
                    Some(text.pos.x)
                }
                _ => None,
            })
            .expect("document marker should remain partially visible")
    };
    for _ in 0..8 {
        run_frame(&context, &mut app, input(vec![]));
    }
    let before = marker_x(&run_frame(&context, &mut app, input(vec![])));
    for _ in 0..12 {
        run_frame(
            &context,
            &mut app,
            input(vec![
                egui::Event::PointerMoved(egui::pos2(500.0, 150.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-3.0, 0.0),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                },
            ]),
        );
    }
    let after = marker_x(&run_frame(&context, &mut app, input(vec![])));
    assert!(
        after < before - 10.0,
        "document did not scroll: {before} -> {after}"
    );
}

#[test]
fn settings_word_wrap_is_enabled_by_default_and_toggles() {
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Empty);
    let output = run_frame(&context, &mut app, input(vec![]));
    let settings = node_with_text(
        accesskit_update(&output),
        egui::accesskit::Role::Button,
        "Settings",
    );
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::Click,
            settings,
            None,
        )]),
    );
    let output = run_frame(&context, &mut app, input(vec![]));
    let wrap = node_with_text(
        accesskit_update(&output),
        egui::accesskit::Role::CheckBox,
        "Word wrap",
    );
    let id = egui::Id::new("word_wrap");
    assert!(context.data(|data| data.get_temp::<bool>(id).unwrap_or(true)));
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::Click,
            wrap,
            None,
        )]),
    );
    assert_eq!(context.data(|data| data.get_temp::<bool>(id)), Some(false));
    run_frame(
        &context,
        &mut app,
        input(vec![accesskit_action(
            egui::accesskit::Action::Click,
            wrap,
            None,
        )]),
    );
    assert_eq!(context.data(|data| data.get_temp::<bool>(id)), Some(true));
}

#[test]
fn settings_selects_solarized_light_and_system_restores_standard_palettes() {
    use fast_markdown_viewer::appearance::ThemeChoice;
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Path(fixture()));
    for (role, label) in [
        (egui::accesskit::Role::Button, "Settings"),
        (egui::accesskit::Role::Button, "Color theme:"),
        (egui::accesskit::Role::RadioButton, "Solarized Light"),
    ] {
        let output = run_frame(&context, &mut app, input(vec![]));
        let node = node_with_text(accesskit_update(&output), role, label);
        run_frame(
            &context,
            &mut app,
            input(vec![accesskit_action(
                egui::accesskit::Action::Click,
                node,
                None,
            )]),
        );
    }
    run_frame(&context, &mut app, input(vec![]));
    assert_eq!(
        context.global_style().visuals.panel_fill,
        egui::Color32::from_rgb(253, 246, 227)
    );
    assert!(!context.global_style().visuals.dark_mode);
    context.set_zoom_factor(1.5);
    for theme in ThemeChoice::ALL {
        theme.apply(&context);
        run_frame(&context, &mut app, input(vec![]));
        assert_eq!(app.tab_count(), 1);
        assert!(app.error_message().is_none());
        assert!((context.zoom_factor() - 1.5).abs() < f32::EPSILON);
    }
    ThemeChoice::System.apply(&context);
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let mut raw = input(vec![]);
        raw.system_theme = Some(theme);
        run_frame(&context, &mut app, raw);
        let expected = if theme == egui::Theme::Light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        assert_eq!(
            context.global_style().visuals.panel_fill,
            expected.panel_fill
        );
    }
}

#[test]
fn top_theme_menu_applies_nord_and_sepia() {
    use egui::accesskit::{Action, Role};

    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Path(fixture()));
    for (name, color) in [("Nord", [46, 52, 64]), ("Sepia", [246, 239, 225])] {
        for (role, label) in [(Role::Button, "Theme:"), (Role::RadioButton, name)] {
            let output = run_frame(&context, &mut app, input(Vec::new()));
            let id = node_with_text(accesskit_update(&output), role, label);
            run_frame(
                &context,
                &mut app,
                input(vec![accesskit_action(Action::Click, id, None)]),
            );
        }
        run_frame(&context, &mut app, input(Vec::new()));
        let [r, g, b] = color;
        assert_eq!(
            context.global_style().visuals.panel_fill,
            egui::Color32::from_rgb(r, g, b)
        );
    }
}

#[test]
fn settings_text_font_selection_updates_rendered_fonts() {
    let (font_label, font_file) = if cfg!(target_os = "windows") {
        ("Georgia", "georgia.ttf")
    } else if cfg!(target_os = "macos") {
        ("Helvetica", "Helvetica.ttc")
    } else {
        ("DejaVu Sans", "DejaVuSans.ttf")
    };
    let context = english_context();
    fonts::install(&context);
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Path(fixture()));
    for (role, label) in [
        (egui::accesskit::Role::Button, "Settings"),
        (egui::accesskit::Role::Button, "Text font:"),
        (egui::accesskit::Role::RadioButton, font_label),
    ] {
        let output = run_frame(&context, &mut app, input(vec![]));
        let node = node_with_text(accesskit_update(&output), role, label);
        run_frame(
            &context,
            &mut app,
            input(vec![accesskit_action(
                egui::accesskit::Action::Click,
                node,
                None,
            )]),
        );
    }
    run_frame(&context, &mut app, input(vec![]));
    context.fonts_mut(|fonts| {
        assert_eq!(
            fonts.definitions().families[&egui::FontFamily::Proportional][0],
            font_file
        );
    });
    assert_eq!(app.tab_count(), 1);
    assert!(has_actual_glyph(
        &context,
        &egui::FontId::proportional(16.0),
        '日'
    ));
}

#[test]
fn document_text_size_reflows_content_without_zooming_the_interface() {
    use fast_markdown_viewer::appearance::set_text_size_percent;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(file, "# Size heading\n\n{}\n\n`Inline marker`\n\n```text\nCode marker\n```\n\n| Column |\n|---|\n| Table marker |", "Paragraph marker with enough words to reflow. ".repeat(12)).unwrap();
    let context = english_context();
    let mut app = ViewerApp::new(InitialState::Path(file.path().to_owned()));
    let mut samples = Vec::new();
    for percent in [100, 150, 150, 100] {
        set_text_size_percent(&context, percent);
        let mut output = egui::FullOutput::default();
        for _ in 0..4 {
            let mut raw = input(vec![]);
            raw.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 1600.0),
            ));
            output = run_frame(&context, &mut app, raw);
        }
        let fonts: Vec<f32> = [
            "Settings",
            "Size heading",
            "Paragraph marker",
            "Inline marker",
            "Code marker",
            "Table marker",
        ]
        .into_iter()
        .map(|marker| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text().starts_with(marker) => {
                        Some(text.galley.job.sections[0].format.font_id.size)
                    }
                    _ => None,
                })
                .reduce(f32::max)
                .unwrap_or_else(|| panic!("missing {marker}"))
        })
        .collect();
        let height = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().starts_with("Paragraph marker") => {
                    Some(text.galley.size().y)
                }
                _ => None,
            })
            .unwrap();
        samples.push((fonts, height));
        assert!((context.zoom_factor() - 1.0).abs() < f32::EPSILON);
    }
    assert!(
        (samples[0].0[0] - samples[1].0[0]).abs() < f32::EPSILON,
        "toolbar size changed"
    );
    for index in 1..samples[0].0.len() {
        assert!((samples[1].0[index] / samples[0].0[index] - 1.5).abs() < 0.01);
    }
    assert!(
        samples[1].1 > samples[0].1 * 1.5,
        "text must reflow in the same width"
    );
    assert_eq!(
        samples[1], samples[2],
        "scaling must not compound across frames"
    );
    assert_eq!(
        samples[0], samples[3],
        "100% must restore the original typography"
    );
}

#[test]
fn settings_text_size_slider_sets_document_size_without_changing_zoom() {
    let context = english_context();
    context.enable_accesskit();
    let mut app = ViewerApp::new(InitialState::Empty);
    let mut raw = input(vec![]);
    raw.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(900.0, 900.0),
    ));
    let output = run_frame(&context, &mut app, raw.clone());
    let settings = node_with_text(
        accesskit_update(&output),
        egui::accesskit::Role::Button,
        "Settings",
    );
    raw.events = vec![accesskit_action(
        egui::accesskit::Action::Click,
        settings,
        None,
    )];
    run_frame(&context, &mut app, raw.clone());
    raw.events.clear();
    let output = run_frame(&context, &mut app, raw.clone());
    let slider = node_with_text(
        accesskit_update(&output),
        egui::accesskit::Role::Slider,
        "Document text size",
    );
    raw.events = vec![accesskit_action(
        egui::accesskit::Action::SetValue,
        slider,
        Some(egui::accesskit::ActionData::NumericValue(150.0)),
    )];
    run_frame(&context, &mut app, raw);
    assert_eq!(
        fast_markdown_viewer::appearance::text_size_percent(&context),
        150
    );
    assert!((context.zoom_factor() - 1.0).abs() < f32::EPSILON);
}
