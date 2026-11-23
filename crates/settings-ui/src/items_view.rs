//! iced view for the remaining Settings items (state in `items`). Kept apart
//! from `window` so other Settings sections can be added next to it.
use crate::ipc::PipeClient;
use crate::items::{
    ItemsSession, LoginItem, MicLister, MicOption, LANGUAGES, RELEASES_URL, SOURCE_URL, VERSION,
};
use aloud_type_config::{
    ConfigFile, Gpu, HorizontalPosition, IndicatorStyle, Language, ModelMemory, Settings, State,
    VerticalPosition,
};
use iced::alignment::Vertical::Center;
use iced::widget::{button, checkbox, column, container, pick_list, row, space, text, toggler};
use iced::Length::Fill;
use iced::{font, Element, Font};
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
    Choice {
        value,
        label: label.into(),
    }
}

#[derive(Debug, Clone)]
pub enum ItemsMsg {
    Microphone(Choice<Option<String>>),
    Language(Choice<Language>),
    LanguageFromKeyboard(bool),
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
        Self {
            session: ItemsSession::new(config, PipeClient::default(), login, mics)
                .with_state(state),
        }
    }

    /// Re-read the settings another part of the window may have written.
    pub fn reload(&mut self) {
        self.session.reload();
    }

    /// Re-read the GPU state; true while the window should keep polling it.
    pub fn poll_gpu(&mut self) -> bool {
        self.session.poll_gpu()
    }

    /// Keep re-reading the GPU state for a while (the resident process is about to write it).
    /// True when polling should start.
    pub fn watch_gpu(&mut self) -> bool {
        self.session.watch_gpu()
    }

    /// Apply a change. True when it started GPU work worth watching (a test or a reload).
    pub fn update(&mut self, m: ItemsMsg) -> bool {
        let gpu_work = matches!(m, ItemsMsg::Gpu(_) | ItemsMsg::RetestGpu);
        match m {
            ItemsMsg::Gpu(c) => self.session.set_gpu(c.value),
            ItemsMsg::RetestGpu => self.session.retest_gpu(),
            ItemsMsg::Microphone(c) => self.session.set_microphone(c.value),
            ItemsMsg::Language(c) => self.session.set_language(c.value),
            ItemsMsg::LanguageFromKeyboard(on) => self.session.set_language_from_keyboard(on),
            ItemsMsg::Style(c) => self.session.set_indicator_style(c.value),
            ItemsMsg::Vertical(c) => self.session.set_vertical(c.value),
            ItemsMsg::Horizontal(c) => self.session.set_horizontal(c.value),
            ItemsMsg::Memory(c) => self.session.set_model_memory(c.value),
            ItemsMsg::Login(on) => self.session.set_launch_at_login(on),
            ItemsMsg::OpenLink(url) => open_link(url),
        }
        gpu_work
    }

    /// The Microphone card.
    pub fn microphone_card(&self) -> Element<'_, ItemsMsg> {
        let s = self.session.settings();
        let mics: Vec<Choice<Option<String>>> = self
            .session
            .mic_options()
            .into_iter()
            .map(|MicOption { id, label }| Choice { value: id, label })
            .collect();
        let mic_sel = mics.iter().find(|c| c.value == s.microphone).cloned();
        card(
            "Microphone",
            pick_list(mics, mic_sel, ItemsMsg::Microphone).width(Fill),
        )
    }

    /// The spoken-language picker (or why there is none), without a heading.
    pub fn language_field(&self) -> Element<'_, ItemsMsg> {
        let s = self.session.settings();
        let mut langs = vec![choice(Language::Auto, "Auto")];
        langs.extend(
            LANGUAGES
                .iter()
                .map(|(c, n)| choice(Language::Fixed((*c).into()), n)),
        );
        let lang_sel = langs.iter().find(|c| c.value == s.language).cloned();
        if self.session.language_enabled() {
            let from_keyboard = s.language_from_keyboard;
            column![
                labelled(
                    if from_keyboard {
                        "Language (when the keyboard's language is not supported)"
                    } else {
                        "Language"
                    },
                    pick_list(langs, lang_sel, ItemsMsg::Language).width(Fill),
                ),
                checkbox(from_keyboard)
                    .label("Use the language of the current keyboard layout")
                    .on_toggle(ItemsMsg::LanguageFromKeyboard)
                    .size(16)
                    .text_size(13),
                muted("Read when you press the Dictation key, so switching the layout (Win+Space) switches the language."),
            ]
            .spacing(6)
            .into()
        } else {
            labelled(
                "Language",
                muted("English (the chosen Speech model is English only)"),
            )
        }
    }

    /// The Listening indicator card: look and screen position.
    pub fn indicator_card(&self) -> Element<'_, ItemsMsg> {
        let s = self.session.settings();
        let styles = vec![
            choice(IndicatorStyle::TraceStrip, "Trace strip"),
            choice(IndicatorStyle::BarPill, "Bar pill"),
            choice(IndicatorStyle::Orb, "Orb"),
        ];
        let style_sel = styles
            .iter()
            .find(|c| c.value == s.indicator_style)
            .cloned();
        let verticals = vec![
            choice(VerticalPosition::Top, "Top"),
            choice(VerticalPosition::Bottom, "Bottom"),
        ];
        let vert_sel = verticals
            .iter()
            .find(|c| c.value == s.indicator_vertical)
            .cloned();
        let horizontals = vec![
            choice(HorizontalPosition::Left, "Left"),
            choice(HorizontalPosition::Center, "Center"),
            choice(HorizontalPosition::Right, "Right"),
        ];
        let horiz_sel = horizontals
            .iter()
            .find(|c| c.value == s.indicator_horizontal)
            .cloned();
        card(
            "Listening indicator",
            row![
                labelled(
                    "Style",
                    pick_list(styles, style_sel, ItemsMsg::Style).width(Fill)
                ),
                labelled(
                    "Screen edge",
                    pick_list(verticals, vert_sel, ItemsMsg::Vertical).width(Fill)
                ),
                labelled(
                    "Alignment",
                    pick_list(horizontals, horiz_sel, ItemsMsg::Horizontal).width(Fill)
                ),
            ]
            .spacing(10),
        )
    }

    /// The Performance card: model memory and GPU.
    pub fn performance_card(&self) -> Element<'_, ItemsMsg> {
        let s = self.session.settings();
        let memories = vec![
            choice(ModelMemory::KeepLoaded, "Keep loaded"),
            choice(ModelMemory::UnloadWhenIdle, "Unload when idle"),
        ];
        let mem_sel = memories.iter().find(|c| c.value == s.model_memory).cloned();
        let gpus: Vec<Choice<Gpu>> = self
            .session
            .gpu_options()
            .into_iter()
            .map(|(value, label)| Choice { value, label })
            .collect();
        let gpu_sel = gpus.iter().find(|c| c.value == s.gpu).cloned();
        let testing = self.session.gpu_testing();
        let test = button(text(if testing { "Testing..." } else { "Test speed" }))
            .style(button::secondary)
            .on_press_maybe((!testing && self.session.has_gpus()).then_some(ItemsMsg::RetestGpu));
        // GPU names are long: the GPU choice gets its own full-width row so its menu is too.
        let mut col = column![
            labelled(
                "Speech model memory",
                pick_list(memories, mem_sel, ItemsMsg::Memory).width(Fill)
            ),
            labelled(
                "GPU",
                row![pick_list(gpus, gpu_sel, ItemsMsg::Gpu).width(Fill), test]
                    .spacing(6)
                    .align_y(Center)
            ),
        ]
        .spacing(10);
        if let Some(n) = self.session.gpu_notice() {
            col = col.push(muted(n));
        }
        for line in self.session.gpu_status() {
            col = col.push(muted(line));
        }
        card("Performance", col)
    }

    /// The General card: start-up, version and links, and any save error.
    pub fn general_card(&self) -> Element<'_, ItemsMsg> {
        let mut col = column![
            toggler(self.session.launch_at_login())
                .label("Launch at login")
                .on_toggle(ItemsMsg::Login),
            row![
                muted(format!("Aloud Type {VERSION}")),
                space::horizontal(),
                button(text("Releases").size(13))
                    .style(button::text)
                    .on_press(ItemsMsg::OpenLink(RELEASES_URL)),
                button(text("Source code").size(13))
                    .style(button::text)
                    .on_press(ItemsMsg::OpenLink(SOURCE_URL)),
            ]
            .spacing(4)
            .align_y(Center),
        ]
        .spacing(10);
        if let Some(e) = self.session.error() {
            col = col.push(
                text(format!("Could not save: {e}"))
                    .size(13)
                    .style(text::danger),
            );
        }
        card("General", col)
    }
}

/// A titled, bordered group of related settings.
pub fn card<'a, M: 'a>(title: &'a str, body: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(
        column![
            text(title).size(16).font(Font {
                weight: font::Weight::Semibold,
                ..Font::DEFAULT
            }),
            body.into()
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .style(container::bordered_box)
    .into()
}

/// A small caption above a control.
pub fn labelled<'a, M: 'a>(label: &'a str, control: impl Into<Element<'a, M>>) -> Element<'a, M> {
    column![muted(label), control.into()]
        .spacing(4)
        .width(Fill)
        .into()
}

/// Secondary, smaller text (help lines, captions, notes).
pub fn muted<'a>(t: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(t).size(12).style(text::secondary)
}

fn open_link(url: &str) {
    let _ = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
}
