//! The Settings window (iced, tiny-skia CPU renderer). It runs in its own
//! short-lived process: closing the window ends the process, which frees all
//! of its memory by construction. Logic lives in `session`; this is only the shell.
use crate::capture::CaptureState;
use crate::codes::{display_binding, vk_for_code_name};
use crate::download::{download_model, DownloadError, UreqHttp};
use crate::ipc::PipeClient;
use crate::manager::{ModelManager, ModelStatus, Selection};
use crate::session::RebindSession;
use aloud_type_config::{AppPaths, ConfigFile};
use aloud_type_models::find;
use iced::widget::{button, column, row, text};
use iced::{keyboard, Element, Subscription, Task};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Progress and outcome shared between a download worker thread and the window.
#[derive(Default)]
struct Shared {
    done: u64,
    total: u64,
    result: Option<Result<(), DownloadError>>,
}

pub const HELP_LINE: &str = "If the key does nothing, another program (a keyboard utility, a game overlay or remote-desktop software) may be intercepting it first. Close that program, or choose a different key.";

#[derive(Debug, Clone)]
enum Message {
    StartCapture,
    Key(keyboard::Event),
    CaptureTick,
    SelectModel(&'static str),
    Download(&'static str),
    DownloadTick,
}

struct App {
    session: RebindSession<PipeClient>,
    started: Instant,
    info: Option<String>,
    warning: Option<String>,
    models: ModelManager<PipeClient>,
    workers: Vec<(&'static str, Arc<Mutex<Shared>>)>,
}

impl App {
    fn boot() -> (App, Task<Message>) {
        let paths = AppPaths::from_env().unwrap_or_else(|| AppPaths::under(std::path::Path::new(".")));
        let app = App {
            session: RebindSession::new(ConfigFile::open(paths.settings_file()), PipeClient::default()),
            started: Instant::now(),
            info: None,
            warning: None,
            models: ModelManager::new(
                paths.models_dir(),
                ConfigFile::open(paths.settings_file()),
                ConfigFile::open(paths.state_file()),
                PipeClient::default(),
            ),
            workers: Vec::new(),
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

    fn begin_download(&mut self, id: &'static str) -> Task<Message> {
        let Some(info) = find(id) else { return Task::none() };
        if self.workers.iter().any(|(w, _)| *w == id) {
            return Task::none();
        }
        self.models.start_download(id);
        let shared = Arc::new(Mutex::new(Shared { done: 0, total: info.size, result: None }));
        self.workers.push((id, shared.clone()));
        let dir = self.models.models_dir().to_path_buf();
        std::thread::spawn(move || {
            let sh = shared.clone();
            let r = download_model(info, &dir, &mut UreqHttp::default(), &mut |d, t| {
                let mut g = sh.lock().unwrap();
                g.done = d;
                g.total = t;
            });
            shared.lock().unwrap().result = Some(r);
        });
        tick_after_millis(300, Message::DownloadTick)
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::SelectModel(id) => {
                if self.models.select(id) == Selection::NeedsDownload {
                    return self.begin_download(id);
                }
            }
            Message::Download(id) => return self.begin_download(id),
            Message::DownloadTick => {
                let mut finished = Vec::new();
                for (id, sh) in &self.workers {
                    let g = sh.lock().unwrap();
                    match g.result {
                        Some(r) => finished.push((*id, r)),
                        None => self.models.progress(id, g.done, g.total),
                    }
                }
                for (id, r) in finished {
                    self.workers.retain(|(w, _)| *w != id);
                    self.models.finish_download(id, r);
                }
                if !self.workers.is_empty() {
                    return tick_after_millis(300, Message::DownloadTick);
                }
            }
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
        col = col.push(text(HELP_LINE).size(12));
        col.push(self.model_section()).into()
    }

    fn model_section(&self) -> Element<'_, Message> {
        let mut col = column![text("Speech model").size(18)].spacing(6);
        if self.models.needs_setup() {
            let m = self.models.first_run_model();
            col = col.push(text("Download a Speech model to start dictating. It is a one-time download; afterwards Aloud Type never uses the network.").size(13));
            col = col.push(
                button(text(format!("Download {} ({})", m.label, crate::manager::size_text(m.size))))
                    .on_press(Message::Download(m.id)),
            );
        }
        for r in self.models.rows() {
            let mark = if r.selected { "(x)" } else { "( )" };
            let name = text(format!("{mark} {}  {}", r.label, r.size_text)).size(14);
            let status: Element<'_, Message> = match &r.status {
                ModelStatus::Downloaded => button(text("Use")).on_press_maybe((!r.selected).then_some(Message::SelectModel(r.id))).into(),
                ModelStatus::NotDownloaded => button(text("Download")).on_press(Message::SelectModel(r.id)).into(),
                ModelStatus::Downloading { done, total } => {
                    text(format!("Downloading {}%", if *total == 0 { 0 } else { done * 100 / total })).size(13).into()
                }
                ModelStatus::Failed(e) => row![text(e.message()).size(13), button(text("Retry")).on_press(Message::Download(r.id))]
                    .spacing(8)
                    .into(),
            };
            col = col.push(row![name, status].spacing(10));
        }
        if !self.models.language_enabled() {
            col = col.push(text("Language: English (this model is English only)").size(13));
        }
        col.into()
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

fn tick_after_millis(ms: u64, m: Message) -> Task<Message> {
    Task::perform(
        async move {
            std::thread::sleep(Duration::from_millis(ms));
        },
        move |_| m.clone(),
    )
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
        .window_size((520.0, 600.0))
        .run()
}
