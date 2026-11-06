#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use eframe::egui;
use fast_markdown_viewer::{
    app::{InitialState, ViewerApp},
    cli::{self, Command},
    fonts, graphics, network, platform,
};

fn main() -> eframe::Result {
    fast_markdown_viewer::mermaid::handle_helper_args();
    let command = cli::parse(std::env::args_os());
    if command == Command::Version {
        println!("FastMarkdownViewer {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    let (_apple_events, inbox) = fmv_macos_events::Registration::install();

    let initial = match command {
        Command::Open(Some(path)) => InitialState::Path(path),
        Command::Open(None) => InitialState::Empty,
        Command::Error(error) => InitialState::Error(error),
        Command::Version => unreachable!(),
    };
    let title = match &initial {
        InitialState::Path(path) => path.file_name().map_or_else(
            || "FastMarkdownViewer".to_owned(),
            |name| format!("{} — FastMarkdownViewer", name.to_string_lossy()),
        ),
        InitialState::Empty | InitialState::Error(_) => "FastMarkdownViewer".to_owned(),
    };
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_app_id("FastMarkdownViewer")
            .with_drag_and_drop(true)
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([420.0, 280.0])
            .with_icon(platform::app_icon()),
        persist_window: false,
        persistence_path: None,
        ..Default::default()
    };

    graphics::run_native(
        "FastMarkdownViewer",
        native_options,
        Box::new(move |creation_context| {
            network::install(&creation_context.egui_ctx);
            fonts::install(&creation_context.egui_ctx);
            fast_markdown_viewer::appearance::ThemeChoice::default()
                .apply(&creation_context.egui_ctx);
            creation_context.egui_ctx.all_styles_mut(|style| {
                style.spacing.item_spacing.y = 7.0;
                style.visuals.selection.stroke.width = 1.0;
            });
            let app = ViewerApp::with_saved_state(initial, &creation_context.egui_ctx);
            #[cfg(target_os = "macos")]
            let app = app.with_native_events(inbox, &creation_context.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
