//! The Settings window (iced, tiny-skia CPU renderer). It runs in its own
//! short-lived process: closing the window ends the process, which frees all
//! of its memory by construction. Logic lives in `session`; this is only the shell.
use crate::capture::CaptureState;
use crate::codes::{display_binding, vk_for_code_name};
use crate::download::{download_model, DownloadError, UreqHttp};
use crate::ipc::PipeClient;
use crate::items_view::{card, labelled, muted, ItemsMsg, ItemsPanel};
use crate::items_win::{WinLogin, WinMics};
use crate::manager::{ModelManager, ModelStatus, Selection};
use crate::session::RebindSession;
use aloud_type_config::{AppPaths, ConfigFile};
use aloud_type_models::{find, is_installed, VAD_MODEL};
use iced::alignment::Horizontal::Right;
use iced::alignment::Vertical::Center;
use iced::widget::{button, column, container, progress_bar, radio, row, scrollable, text};
use iced::Length::{Fill, FillPortion};
use iced::{keyboard, window, Element, Size, Subscription, Task};
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
    Items(ItemsMsg),
    /// Re-read GPU state while a GPU test runs or a GPU change reloads the model.
    GpuTick,
}

struct App {
    session: RebindSession<PipeClient>,
    items: ItemsPanel<WinLogin, WinMics>,
    started: Instant,
    info: Option<String>,
    warning: Option<String>,
    models: ModelManager<PipeClient>,
    workers: Vec<(&'static str, Arc<Mutex<Shared>>)>,
}

impl App {
    fn boot() -> (App, Task<Message>) {
        let paths =
            AppPaths::from_env().unwrap_or_else(|| AppPaths::under(std::path::Path::new(".")));
        let app = App {
            session: RebindSession::new(
                ConfigFile::open(paths.settings_file()),
                PipeClient::default(),
            ),
            items: ItemsPanel::new(
                ConfigFile::open(paths.settings_file()),
                WinLogin::new(),
                WinMics::new(),
            ),
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
        let mut app = app;
        // The resident process may still be saving the GPUs (first launch) or loading the
        // model: re-read its state for a while so the GPU list and status fill in.
        let task = if app.items.watch_gpu() {
            tick_after_millis(1000, Message::GpuTick)
        } else {
            Task::none()
        };
        (app, task)
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
        let Some(info) = find(id) else {
            return Task::none();
        };
        if self.workers.iter().any(|(w, _)| *w == id) {
            return Task::none();
        }
        self.models.start_download(id);
        let shared = Arc::new(Mutex::new(Shared {
            done: 0,
            total: info.size,
            result: None,
        }));
        self.workers.push((id, shared.clone()));
        let dir = self.models.models_dir().to_path_buf();
        std::thread::spawn(move || {
            let sh = shared.clone();
            let r = download_model(info, &dir, &mut UreqHttp::default(), &mut |d, t| {
                let mut g = sh.lock().unwrap();
                g.done = d;
                g.total = t;
            });
            if r.is_ok() && !is_installed(&dir, &VAD_MODEL) {
                // The voice detector rides along; the speech model works without it.
                let _ = download_model(&VAD_MODEL, &dir, &mut UreqHttp::default(), &mut |_, _| {});
            }
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
                // The model decides whether a language can be picked.
                self.items.reload();
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
                for (id, r) in &finished {
                    self.workers.retain(|(w, _)| w != id);
                    self.models.finish_download(id, *r);
                }
                let mut tasks = Vec::new();
                if !finished.is_empty() {
                    // A finished download may have switched to its model, which the resident
                    // process now loads (and says which device it runs on).
                    self.items.reload();
                    if self.items.watch_gpu() {
                        tasks.push(tick_after_millis(1000, Message::GpuTick));
                    }
                }
                if !self.workers.is_empty() {
                    tasks.push(tick_after_millis(300, Message::DownloadTick));
                }
                return Task::batch(tasks);
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
            Message::Items(m) => {
                if self.items.update(m) {
                    return tick_after_millis(1000, Message::GpuTick);
                }
            }
            Message::GpuTick => {
                if self.items.poll_gpu() {
                    return tick_after_millis(1000, Message::GpuTick);
                }
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
        let left = column![self.dictation_card(), self.model_card()].spacing(14);
        let right = column![
            self.items.microphone_card().map(Message::Items),
            self.items.indicator_card().map(Message::Items),
            self.items.performance_card().map(Message::Items),
            self.items.general_card().map(Message::Items),
        ]
        .spacing(14);
        let body = row![left.width(FillPortion(1)), right.width(FillPortion(1))].spacing(14);
        scrollable(container(body).padding(18)).into()
    }

    fn dictation_card(&self) -> Element<'_, Message> {
        let label = if self.session.is_capturing() {
            "Press a key...".to_string()
        } else {
            display_binding(self.session.dictation_key())
        };
        let mut col = column![labelled(
            "Dictation key",
            row![
                button(text(label)).on_press(Message::StartCapture),
                muted("Click, then press the key or combination to use.")
            ]
            .spacing(10)
            .align_y(Center)
        )]
        .spacing(10);
        if self.session.is_capturing() {
            col = col.push(muted(
                "Press the key or combination, then release. Esc cancels.",
            ));
        }
        if let Some(i) = &self.info {
            col = col.push(text(i.clone()).size(13));
        }
        if let Some(w) = &self.warning {
            col = col.push(text(format!("Warning: {w}")).size(13).style(text::warning));
        }
        if let Some(e) = self.session.save_error() {
            col = col.push(
                text(format!("Could not save: {e}"))
                    .size(13)
                    .style(text::danger),
            );
        }
        col = col
            .push(muted(HELP_LINE))
            .push(self.items.language_field().map(Message::Items));
        card("Dictation", col)
    }

    fn model_card(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(6);
        if self.models.needs_setup() {
            let m = self.models.first_run_model();
            col = col.push(text("Download a Speech model to start dictating. It is a one-time download; afterwards Aloud Type never uses the network.").size(13));
            col = col.push(
                button(text(format!(
                    "Download {} ({})",
                    m.label,
                    crate::manager::size_text(m.size)
                )))
                .on_press(Message::Download(m.id)),
            );
        }
        let selected = self.models.rows().iter().find(|r| r.selected).map(|r| r.id);
        for r in self.models.rows() {
            let pick = radio(r.label, r.id, selected, Message::SelectModel)
                .size(16)
                .text_size(14)
                .width(Fill);
            let status: Element<'_, Message> = match &r.status {
                ModelStatus::Downloaded => muted("Downloaded").into(),
                ModelStatus::NotDownloaded => button(text("Download").size(13))
                    .style(button::secondary)
                    .on_press(Message::Download(r.id))
                    .into(),
                ModelStatus::Downloading { done, total } => {
                    let pct = if *total == 0 {
                        0.0
                    } else {
                        *done as f32 * 100.0 / *total as f32
                    };
                    progress_bar(0.0..=100.0, pct).length(90).girth(8).into()
                }
                ModelStatus::Failed(e) => row![
                    text(e.message()).size(12).style(text::danger),
                    button(text("Retry").size(13))
                        .style(button::secondary)
                        .on_press(Message::Download(r.id))
                ]
                .spacing(6)
                .align_y(Center)
                .into(),
            };
            col = col.push(
                row![
                    pick,
                    muted(r.size_text.clone()).width(60),
                    container(status).width(100).align_x(Right),
                ]
                .spacing(8)
                .height(32)
                .align_y(Center),
            );
        }
        col = col.push(muted(
            "Pick a model to use it; one that is not downloaded yet downloads first. Larger models are more accurate but slower.",
        ));
        card("Speech model", col)
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
        .window(window::Settings {
            size: Size::new(940.0, 640.0),
            min_size: Some(Size::new(760.0, 480.0)),
            icon: app_icon(),
            ..window::Settings::default()
        })
        .run()
}

/// The app icon for the title bar and taskbar (decoded by Windows from app.ico).
fn app_icon() -> Option<window::Icon> {
    const PX: u32 = 64;
    let rgba = aloud_type_platform_windows::icon::rgba_from_ico(
        aloud_type_platform_windows::icon::APP_ICO,
        PX,
    )?;
    window::icon::from_rgba(rgba, PX, PX).ok()
}
