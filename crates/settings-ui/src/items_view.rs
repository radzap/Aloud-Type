//! iced view for the remaining Settings items (state in `items`). Kept apart
//! from `window` so other Settings sections can be added next to it.
use crate::ipc::PipeClient;
use crate::items::{
    ItemsSession, LoginItem, MicLister, MicOption, LANGUAGES, RELEASES_URL, SOURCE_URL, VERSION,
};
use aloud_type_config::{
    ConfigFile, Gpu, HorizontalPosition, IndicatorStyle, Language, ModelMemory, Settings, State, VerticalPosition,
};
use iced::widget::{button, column, pick_list, row, text, toggler};
use iced::Element;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Choice<T> {
    pub value: T,
    pub label: String,
}
impl<T> fmt::Display for Choice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}
fn choice<T>(value: T, label: &str) -> Choice<T> {
    Choice { value, label: label.into() }
}

#[derive(Debug, Clone)]
pub enum ItemsMsg {
    Microphone(Choice<Option<String>>),
    Language(Choice<Language>),
    Style(Choice<IndicatorStyle>),
    Vertical(Choice<VerticalPosition>),
    Horizontal(Choice<HorizontalPosition>),
    Memory(Choice<ModelMemory>),
    Login(bool),
    Gpu(Choice<Gpu>),
    RetestGpu,
    OpenLink(&'static str),
}

pub struct ItemsPanel<L: LoginItem, M: MicLister> {
    session: ItemsSession<PipeClient, L, M>,
}

impl<L: LoginItem, M: MicLister> ItemsPanel<L, M> {
    pub fn new(config: ConfigFile<Settings>, login: L, mics: M) -> Self {
        // The state file sits next to the settings file; Settings only reads it.
        let state = ConfigFile::<State>::open(config.path().with_file_name("state.toml"));
        Self { session: ItemsSession::new(config, PipeClient::default(), login, mics).with_state(state) }
    }

    pub fn update(&mut self, m: ItemsMsg) {
        match m {
            ItemsMsg::Gpu(c) => self.session.set_gpu(c.value),
            ItemsMsg::RetestGpu => self.session.retest_gpu(),
            ItemsMsg::Microphone(c) => self.session.set_microphone(c.value),
            ItemsMsg::Language(c) => self.session.set_language(c.value),
            ItemsMsg::Style(c) => self.session.set_indicator_style(c.value),
            ItemsMsg::Vertical(c) => self.session.set_vertical(c.value),
            ItemsMsg::Horizontal(c) => self.session.set_horizontal(c.value),
            ItemsMsg::Memory(c) => self.session.set_model_memory(c.value),
            ItemsMsg::Login(on) => self.session.set_launch_at_login(on),
            ItemsMsg::OpenLink(url) => open_link(url),
        }
    }

    pub fn view(&self) -> Element<'_, ItemsMsg> {
        let gpu_notice = self.session.gpu_notice();
        let gpus: Vec<Choice<Gpu>> =
            self.session.gpu_options().into_iter().map(|(value, label)| Choice { value, label }).collect();
        let gpu_sel = gpus.iter().find(|c| c.value == self.session.settings().gpu).cloned();
        let s = self.session.settings();
        let mics: Vec<Choice<Option<String>>> =
            self.session.mic_options().into_iter().map(|MicOption { id, label }| Choice { value: id, label }).collect();
        let mic_sel = mics.iter().find(|c| c.value == s.microphone).cloned();

        let mut langs = vec![choice(Language::Auto, "Auto")];
        langs.extend(LANGUAGES.iter().map(|(c, n)| choice(Language::Fixed((*c).into()), n)));
        let lang_sel = langs.iter().find(|c| c.value == s.language).cloned();
        let lang_pick: Element<'_, ItemsMsg> = if self.session.language_enabled() {
            pick_list(langs, lang_sel, ItemsMsg::Language).into()
        } else {
            text("English (this Speech model is English only)").size(14).into()
        };

        let styles = vec![
            choice(IndicatorStyle::TraceStrip, "Trace strip"),
            choice(IndicatorStyle::BarPill, "Bar pill"),
            choice(IndicatorStyle::Orb, "Orb"),
        ];
        let style_sel = styles.iter().find(|c| c.value == s.indicator_style).cloned();
        let verticals = vec![choice(VerticalPosition::Top, "Top"), choice(VerticalPosition::Bottom, "Bottom")];
        let vert_sel = verticals.iter().find(|c| c.value == s.indicator_vertical).cloned();
        let horizontals = vec![
            choice(HorizontalPosition::Left, "Left"),
            choice(HorizontalPosition::Center, "Center"),
            choice(HorizontalPosition::Right, "Right"),
        ];
        let horiz_sel = horizontals.iter().find(|c| c.value == s.indicator_horizontal).cloned();
        let memories = vec![
            choice(ModelMemory::KeepLoaded, "Keep loaded"),
            choice(ModelMemory::UnloadWhenIdle, "Unload when idle"),
        ];
        let mem_sel = memories.iter().find(|c| c.value == s.model_memory).cloned();

        let mut col = column![
            text("Microphone").size(18),
            pick_list(mics, mic_sel, ItemsMsg::Microphone),
            text("Language").size(18),
            lang_pick,
            text("Listening indicator").size(18),
            row![
                pick_list(styles, style_sel, ItemsMsg::Style),
                pick_list(verticals, vert_sel, ItemsMsg::Vertical),
                pick_list(horizontals, horiz_sel, ItemsMsg::Horizontal),
            ]
            .spacing(8),
            text("Speech model memory").size(18),
            pick_list(memories, mem_sel, ItemsMsg::Memory),
            text("GPU").size(18),
            row![pick_list(gpus, gpu_sel, ItemsMsg::Gpu), button(text("Re-test")).on_press(ItemsMsg::RetestGpu)]
                .spacing(8),
            text(gpu_notice.unwrap_or("")).size(14),
            toggler(self.session.launch_at_login()).label("Launch at login").on_toggle(ItemsMsg::Login),
            text(format!("Aloud Type {VERSION}")).size(14),
            row![
                button(text("Releases")).on_press(ItemsMsg::OpenLink(RELEASES_URL)),
                button(text("Source code")).on_press(ItemsMsg::OpenLink(SOURCE_URL)),
            ]
            .spacing(8),
        ]
        .spacing(8);
        if let Some(e) = self.session.error() {
            col = col.push(text(format!("Could not save: {e}")).size(14));
        }
        col.into()
    }
}

fn open_link(url: &str) {
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
}
