use eframe::egui;
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Default)]
struct FontState {
    definitions: egui::FontDefinitions,
    attempted: HashSet<&'static str>,
    preferred: [Option<String>; 2],
    available: Vec<FontOption>,
}

#[derive(Clone)]
struct FontOption {
    label: &'static str,
    file: &'static str,
    code: bool,
}

fn installed_choices() -> Vec<FontOption> {
    [
        ("DejaVu Sans", &["DejaVuSans.ttf"][..], false),
        ("DejaVu Sans Mono", &["DejaVuSansMono.ttf"][..], true),
        (
            "Liberation Sans",
            &["LiberationSans-Regular.ttf"][..],
            false,
        ),
        ("Liberation Mono", &["LiberationMono-Regular.ttf"][..], true),
        ("Noto Sans", &["NotoSans-Regular.ttf"][..], false),
        ("Helvetica", &["Helvetica.ttc"][..], false),
        ("Menlo", &["Menlo.ttc"][..], true),
        ("Segoe UI", &["segoeui.ttf"][..], false),
        ("Arial", &["arial.ttf"][..], false),
        ("Calibri", &["calibri.ttf"][..], false),
        ("Georgia", &["georgia.ttf"][..], false),
        ("Times New Roman", &["times.ttf"][..], false),
        ("Consolas", &["consola.ttf"][..], true),
        (
            "Cascadia Code",
            &["CascadiaCode.ttf", "CascadiaCode-Regular.ttf"][..],
            true,
        ),
        (
            "Cascadia Mono",
            &["CascadiaMono.ttf", "CascadiaMono-Regular.ttf"][..],
            true,
        ),
        ("Courier New", &["cour.ttf"][..], true),
        (
            "JetBrains Mono",
            &["JetBrainsMono-Regular.ttf", "JetBrainsMonoNL-Regular.ttf"][..],
            true,
        ),
        (
            "Fira Code",
            &["FiraCode-Regular.ttf", "FiraCode-VF.ttf"][..],
            true,
        ),
    ]
    .into_iter()
    .filter_map(|(label, files, code)| {
        files
            .iter()
            .find(|file| crate::font_paths::find(file).is_some())
            .map(|file| FontOption { label, file, code })
    })
    .collect()
}

fn apply_definitions(context: &egui::Context, state: &FontState) {
    // The saved definitions retain their original fallback order; changing a
    // preference never deletes emoji or lazily installed script fallbacks.
    let mut definitions = state.definitions.clone();
    for (index, family) in [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
        .into_iter()
        .enumerate()
    {
        if let Some(preferred) = &state.preferred[index] {
            let fonts = definitions.families.entry(family).or_default();
            fonts.retain(|font| font != preferred);
            fonts.insert(0, preferred.clone());
        }
    }
    context.set_fonts(definitions);
    crate::appearance::repaint_all(context);
}

fn select(context: &egui::Context, file: Option<&'static str>, code: bool) -> Result<(), String> {
    let Some(mut state) = context.data_mut(|data| data.remove_temp::<FontState>(state_id())) else {
        return Err("Font settings are not initialized.".to_owned());
    };
    let result = if let Some(file) = file {
        if state.definitions.font_data.contains_key(file) || load_system(&mut state, file) {
            state.preferred[usize::from(code)] = Some(file.to_owned());
            Ok(())
        } else {
            Err("This font could not be loaded. The previous font is still selected.".to_owned())
        }
    } else {
        state.preferred[usize::from(code)] = None;
        Ok(())
    };
    if result.is_ok() {
        apply_definitions(context, &state);
    }
    context.data_mut(|data| data.insert_temp(state_id(), state));
    result
}

/// Installed text and code fonts, loaded only when selected.
pub fn menu(ui: &mut egui::Ui) {
    let language = crate::language::Language::current(ui.ctx());
    let default = language.text("Default", "默认");
    let Some(state) = ui.ctx().data(|data| data.get_temp::<FontState>(state_id())) else {
        return;
    };
    for (code, title) in [(false, language.text("Text font", "正文字体")), (true, language.text("Code font", "代码字体"))] {
        let current = &state.preferred[usize::from(code)];
        let label = state
            .available
            .iter()
            .find(|font| Some(font.file) == current.as_deref())
            .map_or(default, |font| font.label);
        ui.menu_button(format!("{title}: {label}"), |ui| {
            let mut choice = None;
            if ui.radio(current.is_none(), default).clicked() {
                choice = Some(None);
            }
            for font in state.available.iter().filter(|font| font.code == code) {
                if ui
                    .radio(current.as_deref() == Some(font.file), font.label)
                    .clicked()
                {
                    choice = Some(Some(font.file));
                }
            }
            ui.separator();
            ui.weak(language.text("Installed fonts · remembered on exit", "已安装的字体 · 退出时保存"));
            if let Some(choice) = choice {
                let error = select(ui.ctx(), choice, code).err().unwrap_or_default();
                ui.ctx()
                    .data_mut(|data| data.insert_temp(egui::Id::new("font_choice_error"), error));
                ui.close();
            }
        });
    }
    if let Some(error) = ui
        .ctx()
        .data(|data| data.get_temp::<String>(egui::Id::new("font_choice_error")))
        && !error.is_empty()
    {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
}

fn state_id() -> egui::Id {
    egui::Id::new("document_font_fallbacks")
}

pub(crate) fn preferences(context: &egui::Context) -> [Option<String>; 2] {
    context
        .data(|data| data.get_temp::<FontState>(state_id()))
        .map_or([None, None], |state| state.preferred)
}

/// Read only the selected text font; avoid cloning the font database each frame.
pub(crate) fn diagram_font(context: &egui::Context) -> Option<String> {
    context.data_mut(|data| {
        data.get_temp_mut_or_default::<FontState>(state_id())
            .preferred[0]
            .clone()
    })
}

pub(crate) fn restore(context: &egui::Context, preferences: &[Option<String>; 2]) {
    let Some(state) = context.data(|data| data.get_temp::<FontState>(state_id())) else {
        return;
    };
    for (index, preferred) in preferences.iter().enumerate() {
        if let Some(font) = state
            .available
            .iter()
            .find(|font| Some(font.file) == preferred.as_deref() && font.code == (index == 1))
        {
            let _ = select(context, Some(font.file), index == 1);
        }
    }
}

/// Install emoji and small system fallbacks. Large East Asian fonts load only when needed.
pub fn install(context: &egui::Context) {
    let mut state = FontState {
        definitions: egui::FontDefinitions::default(),
        attempted: HashSet::new(),
        preferred: [None, None],
        available: installed_choices(),
    };
    add(
        &mut state.definitions,
        "Noto Emoji",
        egui::FontData::from_static(include_bytes!("../assets/fonts/NotoEmoji-Variable.ttf")),
    );
    for name in ["segoeui.ttf", "arial.ttf", "DejaVuSans.ttf", "Arial.ttf"] {
        load_system(&mut state, name);
    }
    apply_definitions(context, &state);
    context.data_mut(|data| data.insert_temp(state_id(), state));
}

fn add(definitions: &mut egui::FontDefinitions, name: &str, data: egui::FontData) {
    definitions
        .font_data
        .insert(name.to_owned(), Arc::new(data));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        definitions
            .families
            .entry(family)
            .or_default()
            .push(name.to_owned());
    }
}

fn load_system(state: &mut FontState, name: &'static str) -> bool {
    if !state.attempted.insert(name) {
        return false;
    }
    let Some(path) = crate::font_paths::find(name) else {
        return false;
    };
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    add(
        &mut state.definitions,
        name,
        egui::FontData::from_owned(bytes),
    );
    true
}

fn load_system_family(state: &mut FontState, names: &[&'static str]) -> bool {
    for name in names {
        if state.definitions.font_data.contains_key(*name) {
            return false;
        }
        if load_system(state, name) {
            return true;
        }
    }
    false
}

/// Extend both proportional and code fonts for scripts present in the document.
pub fn ensure_for_text(context: &egui::Context, text: &str) {
    let Some(mut state) = context.data_mut(|data| data.remove_temp::<FontState>(state_id())) else {
        return;
    };
    let mut changed = false;
    let mut japanese = false;
    let mut chinese = false;
    let mut korean = false;
    let mut indic = false;
    let mut thai_lao = false;
    for character in text.chars() {
        match character {
            '\u{0900}'..='\u{0dff}' | '\u{a8e0}'..='\u{a8ff}' => indic = true,
            '\u{0e00}'..='\u{0eff}' => thai_lao = true,
            '\u{3040}'..='\u{30ff}' | '\u{31f0}'..='\u{31ff}' => japanese = true,
            '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{323af}' => {
                chinese = true;
            }
            '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' | '\u{ac00}'..='\u{d7af}' => {
                korean = true;
            }
            _ => {}
        }
    }
    if japanese {
        changed |= load_system_family(
            &mut state,
            &[
                "msgothic.ttc",
                "NotoSansCJK-Regular.ttc",
                "Hiragino Sans GB.ttc",
                "PingFang.ttc",
                "STHeiti Light.ttc",
                "STHeiti Medium.ttc",
            ],
        );
    }
    if chinese {
        changed |= load_system_family(
            &mut state,
            &[
                "msyh.ttc",
                "NotoSansCJK-Regular.ttc",
                "PingFang.ttc",
                "STHeiti Light.ttc",
                "STHeiti Medium.ttc",
            ],
        );
    }
    if korean {
        changed |= load_system_family(
            &mut state,
            &[
                "malgun.ttf",
                "NotoSansCJK-Regular.ttc",
                "AppleSDGothicNeo.ttc",
            ],
        );
    }
    if indic {
        changed |= load_system_family(
            &mut state,
            &[
                "Nirmala.ttc",
                "Nirmala.ttf",
                "mangal.ttf",
                "NotoSansDevanagari-Regular.ttf",
                "Devanagari Sangam MN.ttc",
            ],
        );
    }
    if thai_lao {
        changed |= load_system_family(
            &mut state,
            &[
                "LeelawUI.ttf",
                "leelawad.ttf",
                "NotoSansThai-Regular.ttf",
                "Thonburi.ttc",
            ],
        );
    }
    if changed {
        apply_definitions(context, &state);
    }
    context.data_mut(|data| data.insert_temp(state_id(), state));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(context: &egui::Context) {
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            ui.label("Reading 日本語 中文 한국어 العربية עברית 🙂 हिन्दी ภาษาไทย");
            ui.monospace("let answer = 42;");
        });
        output.textures_delta.clear();
    }

    #[test]
    fn font_changes_preserve_independent_families_and_late_fallbacks() {
        #[cfg(target_os = "windows")]
        let (text_font, code_font) = ("georgia.ttf", "consola.ttf");
        #[cfg(target_os = "macos")]
        let (text_font, code_font) = ("Helvetica.ttc", "Menlo.ttc");
        #[cfg(target_os = "linux")]
        let (text_font, code_font) = ("DejaVuSans.ttf", "DejaVuSansMono.ttf");
        let context = egui::Context::default();
        install(&context);
        frame(&context);
        let original = context.fonts_mut(|fonts| fonts.definitions().families.clone());
        select(&context, Some(text_font), false).unwrap();
        frame(&context);
        context.fonts_mut(|fonts| {
            assert_eq!(
                fonts.definitions().families[&egui::FontFamily::Proportional][0],
                text_font
            );
            assert_eq!(
                fonts.definitions().families[&egui::FontFamily::Monospace][0],
                original[&egui::FontFamily::Monospace][0]
            );
        });
        select(&context, Some(code_font), true).unwrap();
        let saved = preferences(&context);
        let reopened = egui::Context::default();
        install(&reopened);
        restore(&reopened, &saved);
        frame(&reopened);
        assert_eq!(preferences(&reopened), saved);
        let missing = egui::Context::default();
        install(&missing);
        restore(
            &missing,
            &[Some("font-no-longer-installed.ttf".into()), None],
        );
        assert_eq!(preferences(&missing), [None, None]);
        ensure_for_text(&context, "日本語 中文 한국어 हिन्दी ภาษาไทย");
        frame(&context);
        context.fonts_mut(|fonts| {
            let definitions = fonts.definitions();
            assert_eq!(
                definitions.families[&egui::FontFamily::Proportional][0],
                text_font
            );
            assert_eq!(
                definitions.families[&egui::FontFamily::Monospace][0],
                code_font
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                for character in "日中한عא🙂हภ".chars() {
                    assert!(
                        definitions.families[&family].iter().any(|name| {
                            let data = &definitions.font_data[name];
                            ttf_parser::Face::parse(&data.font, data.index)
                                .is_ok_and(|face| face.glyph_index(character).is_some())
                        }),
                        "lost fallback for {character}"
                    );
                }
            }
        });
        assert!(select(&context, Some("fmv-intentionally-missing.ttf"), false).is_err());
        frame(&context);
        context.fonts_mut(|fonts| {
            assert_eq!(
                fonts.definitions().families[&egui::FontFamily::Proportional][0],
                text_font
            );
        });
        select(&context, None, false).unwrap();
        select(&context, None, true).unwrap();
        frame(&context);
        context.fonts_mut(|fonts| {
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                assert_eq!(
                    fonts.definitions().families[&family][0],
                    original[&family][0]
                );
            }
        });
    }
}
