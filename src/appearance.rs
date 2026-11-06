//! Small built-in palettes inspired by familiar editor themes.
use eframe::egui::{self, Color32};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DocumentWidth {
    #[default]
    Comfortable,
    FitWindow,
}

impl DocumentWidth {
    #[must_use]
    pub fn current(context: &egui::Context) -> Self {
        context.data(|data| {
            data.get_temp(egui::Id::new("document_width"))
                .unwrap_or_default()
        })
    }

    pub fn apply(self, context: &egui::Context) {
        context.data_mut(|data| data.insert_temp(egui::Id::new("document_width"), self));
        repaint_all(context);
    }

    pub(crate) fn available(self, width: f32) -> f32 {
        match self {
            Self::Comfortable => width.min(960.0),
            Self::FitWindow => width,
        }
        .max(1.0)
    }

    pub(crate) fn menu(ui: &mut egui::Ui) {
        let language = crate::language::Language::current(ui.ctx());
        let previous = Self::current(ui.ctx());
        let mut width = previous;
        let label = |width| match width {
            Self::Comfortable => language.text("Comfortable (960)", "适中 (960)"),
            Self::FitWindow => language.text("Fit window", "适应窗口"),
        };
        ui.menu_button(format!("{}: {}", language.text("Document width", "正文宽度"), label(width)), |ui| {
            for choice in [Self::Comfortable, Self::FitWindow] {
                if ui.radio_value(&mut width, choice, label(choice)).clicked() {
                    ui.close();
                }
            }
        });
        if width != previous {
            width.apply(ui.ctx());
        }
    }
}

pub(crate) const MIN_TEXT_SIZE: u16 = 75;
pub(crate) const MAX_TEXT_SIZE: u16 = 200;

/// Document typography, independent of interface zoom and image dimensions.
#[must_use]
pub fn text_size_percent(context: &egui::Context) -> u16 {
    context.data(|data| {
        data.get_temp(egui::Id::new("document_text_size"))
            .unwrap_or(100)
    })
}

pub fn set_text_size_percent(context: &egui::Context, percent: u16) {
    context.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("document_text_size"),
            percent.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE),
        )
    });
    repaint_all(context);
}

pub(crate) fn text_size_menu(ui: &mut egui::Ui) {
    let language = crate::language::Language::current(ui.ctx());
    let mut percent = text_size_percent(ui.ctx());
    if ui.add(egui::Slider::new(&mut percent, MIN_TEXT_SIZE..=MAX_TEXT_SIZE)
        .text(language.text("Document text size", "正文字号")).suffix("%"))
        .on_hover_text(language.text("Resize prose, headings, tables, and code. Click the number to type a value. Images and Mermaid diagrams keep their size.", "调整正文、标题、表格和代码的字号。点击数字可输入数值。图片和 Mermaid 图表保持原大小。"))
        .changed() {
        set_text_size_percent(ui.ctx(), percent);
    }
}

pub(crate) fn scale_document_text(ui: &mut egui::Ui) {
    let percent = text_size_percent(ui.ctx());
    if percent != 100 {
        let scale = f32::from(percent) / 100.0;
        for font in ui.style_mut().text_styles.values_mut() {
            font.size *= scale;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
    SolarizedLight,
    SolarizedDark,
    QuietLight,
    #[default]
    Monokai,
    TomorrowNightBlue,
}

impl ThemeChoice {
    pub const ALL: [Self; 8] = [
        Self::System,
        Self::Light,
        Self::Dark,
        Self::SolarizedLight,
        Self::SolarizedDark,
        Self::QuietLight,
        Self::Monokai,
        Self::TomorrowNightBlue,
    ];

    #[must_use]
    pub(crate) fn localized_label(self, language: crate::language::Language) -> &'static str {
        let chinese = match self {
            Self::System => "跟随系统",
            Self::Light => "浅色",
            Self::Dark => "深色",
            Self::SolarizedLight => "Solarized 浅色",
            Self::SolarizedDark => "Solarized 深色",
            _ => self.label(),
        };
        language.text(self.label(), chinese)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "Use system setting",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::SolarizedLight => "Solarized Light",
            Self::SolarizedDark => "Solarized Dark",
            Self::QuietLight => "Quiet Light",
            Self::Monokai => "Monokai",
            Self::TomorrowNightBlue => "Tomorrow Night Blue",
        }
    }

    /// Apply a palette to the shared context without changing zoom or fonts.
    pub fn apply(self, context: &egui::Context) {
        // Restore BOTH variants so returning to System can never retain a custom palette.
        context.style_mut_of(egui::Theme::Light, |s| s.visuals = egui::Visuals::light());
        context.style_mut_of(egui::Theme::Dark, |s| s.visuals = egui::Visuals::dark());
        let dark = matches!(
            self,
            Self::Dark | Self::SolarizedDark | Self::Monokai | Self::TomorrowNightBlue
        );
        context.set_theme(if self == Self::System {
            egui::ThemePreference::System
        } else if dark {
            egui::ThemePreference::Dark
        } else {
            egui::ThemePreference::Light
        });
        // background, surface, text, muted text, accent, selected background
        let palette = match self {
            Self::SolarizedLight => Some([
                0xfd_f6_e3, 0xee_e8_d5, 0x65_7b_83, 0x58_6e_75, 0x26_8b_d2, 0xdf_ca_88,
            ]),
            Self::SolarizedDark => Some([
                0x00_2b_36, 0x07_36_42, 0x83_94_96, 0x93_a1_a1, 0x26_8b_d2, 0x16_46_53,
            ]),
            Self::QuietLight => Some([
                0xf5_f5_f5, 0xe8_e8_e8, 0x33_33_33, 0x62_62_62, 0x70_56_97, 0xc9_d0_d9,
            ]),
            Self::Monokai => Some([
                0x27_28_22, 0x34_35_2f, 0xf8_f8_f2, 0xb0_b0_9b, 0xa6_e2_2e, 0x49_48_3e,
            ]),
            Self::TomorrowNightBlue => Some([
                0x00_24_51, 0x00_1c_40, 0xff_ff_ff, 0xa8_b7_d0, 0x99_ff_ff, 0x00_3f_8e,
            ]),
            _ => None,
        };
        if let Some(colors) = palette {
            let [background, surface, text, muted, accent, selection] = colors.map(rgb);
            let mut visuals = if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            visuals.panel_fill = background;
            visuals.window_fill = surface;
            visuals.extreme_bg_color = background;
            visuals.text_edit_bg_color = Some(background);
            visuals.faint_bg_color = surface;
            visuals.code_bg_color = surface;
            visuals.override_text_color = Some(text);
            visuals.weak_text_color = Some(muted);
            visuals.hyperlink_color = accent;
            visuals.selection.bg_fill = selection;
            visuals.selection.stroke = egui::Stroke::new(1.0, text);
            visuals.window_stroke = egui::Stroke::new(1.0, muted.gamma_multiply(0.5));
            for widget in [
                &mut visuals.widgets.noninteractive,
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
                &mut visuals.widgets.open,
            ] {
                widget.bg_fill = surface;
                widget.weak_bg_fill = surface;
                widget.fg_stroke.color = text;
                widget.bg_stroke.color = muted.gamma_multiply(0.5);
            }
            visuals.widgets.hovered.bg_fill = selection;
            visuals.widgets.hovered.weak_bg_fill = selection;
            visuals.widgets.hovered.bg_stroke.color = accent;
            visuals.widgets.active.bg_fill = selection;
            visuals.widgets.active.weak_bg_fill = selection;
            visuals.widgets.active.bg_stroke.color = accent;
            context.style_mut_of(
                if dark {
                    egui::Theme::Dark
                } else {
                    egui::Theme::Light
                },
                |s| s.visuals = visuals,
            );
        }
        egui_commonmark_backend::set_syntax_theme(
            context,
            match self {
                Self::SolarizedLight => "Solarized (light)",
                Self::SolarizedDark => "Solarized (dark)",
                Self::QuietLight => "InspiredGitHub",
                Self::Monokai => "Monokai",
                Self::TomorrowNightBlue => "Tomorrow Night Blue",
                _ => "auto",
            },
        );
        repaint_all(context);
    }
}

const fn rgb(value: u32) -> Color32 {
    let [_, r, g, b] = value.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

pub(crate) fn repaint_all(context: &egui::Context) {
    let viewports = context.input(|input| input.raw.viewports.keys().copied().collect::<Vec<_>>());
    for viewport in viewports {
        context.request_repaint_of(viewport);
    }
    context.request_repaint();
}
