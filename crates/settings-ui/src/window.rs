//! The Settings window (iced, tiny-skia CPU renderer). It runs in its own
//! short-lived process: closing the window ends the process, which frees all
//! of its memory by construction. Logic lives in `session`; this is only the shell.
use crate::capture::CaptureState;
use crate::codes::{display_binding, vk_for_code_name};
use crate::ipc::PipeClient;
use crate::session::RebindSession;
use aloud_type_config::{AppPaths, ConfigFile};
use crate::items_view::{ItemsMsg, ItemsPanel};
use crate::items_win::{WinLogin, WinMics};
use iced::widget::{button, column, row, scrollable, text};
use iced::{keyboard, Element, Subscription, Task};
use std::time::{Duration, Instant};

pub const HELP_LINE: &str = "If the key does nothing, another program (a keyboard utility, a game overlay or remote-desktop software) may be intercepting it first. Close that program, or choose a different key.";

#[derive(Debug, Clone)]
enum Message {
    StartCapture,
    Key(keyboard::Event),
    CaptureTick,
    Items(ItemsMsg),
}

struct App {
    session: RebindSession<PipeClient>,
    items: ItemsPanel<WinLogin, WinMics>,
    started: Instant,
    info: Option<String>,
    warning: Option<String>,
}

impl App {
    fn boot() -> (App, Task<Message>) {
        let path = AppPaths::from_env()
            .map(|p| p.settings_file())
            .unwrap_or_else(|| std::path::PathBuf::from("settings.toml"));
        let app = App {
            session: RebindSession::new(ConfigFile::open(&path), PipeClient::default()),
            items: ItemsPanel::new(ConfigFile::open(&path), WinLogin::new(), WinMics::new()),
            started: Instant::now(),
            info: None,
            warning: None,
        };
        (app, Task::none())
    }

    fn now(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn apply(&mut self, st: Option<CaptureState>) {
        match st {
            Some(CaptureState::Capturing { message, .. }) => self.info = message,
            Some(CaptureState::Done { warning, .. }) => {
                self.info = None;
                self.warning = warning;
            }
            Some(CaptureState::Cancelled(_)) => self.info = None,
            None => {}
        }
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::StartCapture => {
                self.warning = None;
                let now = self.now();
                self.session.begin_capture(now);
                self.info = None;
                return tick_after(Duration::from_secs(1));
            }
            Message::Key(e) => {
                let now = self.now();
                let st = match e {
                    keyboard::Event::KeyPressed { physical_key, .. } => {
                        code_vk(&physical_key).and_then(|vk| self.session.key_down(vk))
                    }
                    keyboard::Event::KeyReleased { physical_key, .. } => {
                        code_vk(&physical_key).and_then(|vk| self.session.key_up(vk, now))
                    }
                    _ => None,
                };
                self.apply(st);
            }
            Message::Items(m) => self.items.update(m),
            Message::CaptureTick => {
                let now = self.now();
                let st = self.session.tick(now);
                self.apply(st);
                if self.session.is_capturing() {
                    return tick_after(Duration::from_secs(1));
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let label = if self.session.is_capturing() {
            "Press a key...".to_string()
        } else {
            display_binding(self.session.dictation_key())
        };
        let mut col = column![
            text("Dictation key").size(18),
            row![button(text(label)).on_press(Message::StartCapture)].spacing(8),
        ]
        .spacing(10)
        .padding(20);
        if self.session.is_capturing() {
            col = col.push(text("Press the key or combination, then release. Esc cancels.").size(13));
        }
        if let Some(i) = &self.info {
            col = col.push(text(i.clone()).size(14));
        }
        if let Some(w) = &self.warning {
            col = col.push(text(format!("Warning: {w}")).size(14));
        }
        if let Some(e) = self.session.save_error() {
            col = col.push(text(format!("Could not save: {e}")).size(14));
        }
        let col = col.push(text(HELP_LINE).size(12)).push(self.items.view().map(Message::Items));
        scrollable(col).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.session.is_capturing() {
            keyboard::listen().map(Message::Key)
        } else {
            Subscription::none()
        }
    }
}

fn code_vk(p: &keyboard::key::Physical) -> Option<u32> {
    match p {
        keyboard::key::Physical::Code(c) => vk_for_code_name(&format!("{c:?}")),
        _ => None,
    }
}

fn tick_after(d: Duration) -> Task<Message> {
    Task::perform(
        async move {
            std::thread::sleep(d);
        },
        |_| Message::CaptureTick,
    )
}

/// Runs the Settings window until it is closed.
pub fn run() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Aloud Type Settings")
        .subscription(App::subscription)
        .window_size((480.0, 640.0))
        .run()
}
