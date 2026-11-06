use eframe::egui;
use std::sync::OnceLock;

#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    #[default]
    System,
    English,
    Chinese,
}

impl Language {
    fn chinese(self) -> bool {
        static SYSTEM_CHINESE: OnceLock<bool> = OnceLock::new();
        match self {
            Self::System => *SYSTEM_CHINESE.get_or_init(|| {
                sys_locale::get_locale().is_some_and(|locale| {
                    locale
                        .split(['-', '_'])
                        .next()
                        .is_some_and(|code| code.eq_ignore_ascii_case("zh"))
                })
            }),
            Self::English => false,
            Self::Chinese => true,
        }
    }

    pub(crate) fn current(context: &egui::Context) -> Self {
        context.data(|data| {
            data.get_temp(egui::Id::new("interface_language"))
                .unwrap_or_default()
        })
    }

    /// Set the shared interface language for this viewer context.
    pub fn apply(self, context: &egui::Context) {
        if self.chinese() {
            crate::fonts::ensure_for_text(context, "简体中文");
        }
        context.data_mut(|data| data.insert_temp(egui::Id::new("interface_language"), self));
        crate::appearance::repaint_all(context);
    }

    pub(crate) fn text(self, english: &'static str, chinese: &'static str) -> &'static str {
        if self.chinese() { chinese } else { english }
    }

    pub(crate) fn menu(ui: &mut egui::Ui) {
        let current = Self::current(ui.ctx());
        let mut selected = current;
        ui.menu_button(current.text("Language", "语言"), |ui| {
            for (choice, label) in [
                (Self::System, current.text("Follow system", "跟随系统")),
                (Self::English, "English"),
                (
                    Self::Chinese,
                    current.text("Simplified Chinese", "简体中文"),
                ),
            ] {
                ui.radio_value(&mut selected, choice, label);
            }
        });
        if selected != current {
            selected.apply(ui.ctx());
        }
    }
}
