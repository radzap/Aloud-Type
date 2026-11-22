//! Model residency and swapping: the Speech model loads on a worker thread,
//! can be unloaded when idle, and is swapped for another without ever
//! interrupting a Dictation. Engine-agnostic: models come from a `Loader`.
use aloud_type_config::ModelMemory;
use aloud_type_core::{Millis, Recognizer, Segment};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Freed this long after the last Dictation when "Unload when idle" is set.
pub const IDLE_UNLOAD_MS: Millis = 10 * 60 * 1000;

pub type Model = Box<dyn Recognizer + Send>;
/// Loads the Speech model with the given id (runs on a worker thread).
pub type Loader = Arc<dyn Fn(&str) -> Result<Model, String> + Send + Sync>;
pub type NowFn = Arc<dyn Fn() -> Millis + Send + Sync>;
pub type Waker = Arc<dyn Fn() + Send + Sync>;

struct State {
    wanted: String,
    memory: ModelMemory,
    model: Option<(String, Model)>,
    /// Id of the load in flight (a stale result is dropped by generation).
    loading: Option<String>,
    generation: u64,
    /// A swap target that finished loading, waiting for a Dictation-free moment.
    staged: Option<(String, Model)>,
    dictating: bool,
    last_activity: Millis,
    notices: Vec<String>,
    load_failed: bool,
    waker: Option<Waker>,
}

struct Inner {
    state: Mutex<State>,
    loaded: Condvar,
    loader: Loader,
    now: NowFn,
}

/// Shared handle to the resident model's lifecycle. Cheap to clone.
#[derive(Clone)]
pub struct Residency(Arc<Inner>);

impl Residency {
    /// Start loading `model_id` on a worker thread right away.
    pub fn start(loader: Loader, now: NowFn, model_id: &str, memory: ModelMemory) -> Self {
        let t = now();
        let r = Residency(Arc::new(Inner {
            state: Mutex::new(State {
                wanted: model_id.into(),
                memory,
                model: None,
                loading: None,
                generation: 0,
                staged: None,
                dictating: false,
                last_activity: t,
                notices: vec![],
                load_failed: false,
                waker: None,
            }),
            loaded: Condvar::new(),
            loader,
            now,
        }));
        let mut st = r.lock();
        r.begin_load(&mut st, model_id);
        drop(st);
        r
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn begin_load(&self, st: &mut State, id: &str) {
        st.generation += 1;
        st.loading = Some(id.to_string());
        st.load_failed = false;
        let generation = st.generation;
        let me = self.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            let result = (me.0.loader)(&id);
            let mut st = me.lock();
            if st.generation == generation {
                st.loading = None;
                match result {
                    Ok(m) if st.model.is_none() => st.model = Some((id, m)),
                    Ok(m) => st.staged = Some((id, m)),
                    Err(e) => {
                        if let Some((cur, _)) = &st.model {
                            let msg = format!(
                                "Could not load Speech model \"{id}\" ({e}); keeping \"{cur}\""
                            );
                            st.notices.push(msg);
                        } else {
                            st.load_failed = true;
                            st.notices
                                .push(format!("Could not load Speech model \"{id}\": {e}"));
                        }
                    }
                }
            }
            let waker = st.waker.clone();
            drop(st);
            me.0.loaded.notify_all();
            if let Some(w) = waker {
                w();
            }
        });
    }

    fn apply_staged(st: &mut State) {
        if !st.dictating {
            if let Some(s) = st.staged.take() {
                st.model = Some(s);
            }
        }
    }

    /// A Dictation started: reload in parallel with capture if unloaded.
    pub fn dictation_started(&self) {
        let mut st = self.lock();
        st.dictating = true;
        if st.model.is_none() && st.loading.is_none() {
            let id = st.wanted.clone();
            self.begin_load(&mut st, &id);
        }
    }

    /// A Dictation ended: the idle clock restarts and a finished swap lands.
    pub fn dictation_ended(&self) {
        let mut st = self.lock();
        st.dictating = false;
        st.last_activity = (self.0.now)();
        Self::apply_staged(&mut st);
    }

    /// Housekeeping: lands a finished swap and frees the model when idle long enough.
    pub fn tick(&self) {
        let mut st = self.lock();
        Self::apply_staged(&mut st);
        if st.memory == ModelMemory::UnloadWhenIdle
            && !st.dictating
            && st.model.is_some()
            && (self.0.now)().saturating_sub(st.last_activity) >= IDLE_UNLOAD_MS
        {
            st.model = None;
        }
    }

    /// Time until the model is freed for being idle, if that is pending.
    pub fn idle_unload_in(&self) -> Option<Millis> {
        let st = self.lock();
        if st.memory != ModelMemory::UnloadWhenIdle || st.dictating || st.model.is_none() {
            return None;
        }
        Some(IDLE_UNLOAD_MS.saturating_sub((self.0.now)().saturating_sub(st.last_activity)))
    }

    /// Called (from the loading thread) whenever a load finishes, so a sleeping
    /// host can pick up the model, a staged swap or a notice.
    pub fn set_waker(&self, waker: Waker) {
        self.lock().waker = Some(waker);
    }

    /// Change the residency mode; takes effect immediately.
    pub fn set_memory(&self, memory: ModelMemory) {
        let mut st = self.lock();
        st.memory = memory;
        if memory == ModelMemory::KeepLoaded && st.model.is_none() && st.loading.is_none() {
            let id = st.wanted.clone();
            self.begin_load(&mut st, &id);
        }
    }

    /// Switch to another model: it loads while the current one keeps serving.
    pub fn request_model(&self, id: &str) {
        let mut st = self.lock();
        if st.wanted == id {
            return;
        }
        st.wanted = id.to_string();
        let current = st.model.as_ref().map(|(c, _)| c.clone());
        match current {
            Some(c) if c == id => {
                // Back to what is loaded: forget any swap in progress.
                st.generation += 1;
                st.loading = None;
                st.staged = None;
            }
            Some(_) => {
                st.staged = None;
                self.begin_load(&mut st, id);
            }
            None => {
                // Unloaded (or the first load is in flight, or it failed).
                if st.loading.is_some() || st.memory == ModelMemory::KeepLoaded || st.dictating {
                    self.begin_load(&mut st, id);
                }
            }
        }
    }

    /// Load the wanted model again because how it loads changed (the GPU choice or a GPU
    /// test). The current model keeps serving until the new copy is ready; a load in flight is
    /// restarted. An unloaded model picks up the change at its next load.
    pub fn reload(&self) {
        let mut st = self.lock();
        if st.model.is_some() || st.loading.is_some() {
            st.staged = None;
            let id = st.wanted.clone();
            self.begin_load(&mut st, &id);
        }
    }

    /// Notices to show the user (failed loads), drained.
    pub fn take_notices(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().notices)
    }

    /// Id of the model currently serving, if loaded.
    pub fn loaded_model(&self) -> Option<String> {
        self.lock().model.as_ref().map(|(i, _)| i.clone())
    }

    pub fn is_loading(&self) -> bool {
        self.lock().loading.is_some()
    }

    /// The recognizer view of this model: blocks until a model is ready.
    pub fn recognizer(&self) -> ResidentRecognizer {
        ResidentRecognizer(self.clone())
    }
}

pub struct ResidentRecognizer(Residency);

impl Recognizer for ResidentRecognizer {
    fn recognize(&mut self, audio: &[f32], language: &str, prompt: Option<&str>) -> Vec<Segment> {
        let r = &self.0;
        let mut st = r.lock();
        let (id, mut model) = loop {
            Residency::apply_staged(&mut st);
            if let Some(m) = st.model.take() {
                break m;
            }
            if st.loading.is_none() {
                if st.load_failed {
                    return Vec::new();
                }
                let id = st.wanted.clone();
                r.begin_load(&mut st, &id);
            }
            // The load is in flight: the caller's audio stays buffered meanwhile.
            st = r.0.loaded.wait(st).unwrap_or_else(|e| e.into_inner());
        };
        drop(st);
        let out = model.recognize(audio, language, prompt);
        let mut st = r.lock();
        if st.model.is_none() {
            st.model = Some((id, model));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    /// Speaks as the model it was loaded as.
    struct Named(String);
    impl Recognizer for Named {
        fn recognize(&mut self, _: &[f32], _: &str, _: Option<&str>) -> Vec<Segment> {
            vec![Segment {
                text: self.0.clone(),
                words: vec![],
                no_speech_prob: 0.0,
            }]
        }
    }

    #[derive(Clone, Default)]
    struct Gate(Arc<(Mutex<bool>, Condvar)>);
    impl Gate {
        fn open(&self) {
            *self.0 .0.lock().unwrap() = true;
            self.0 .1.notify_all();
        }
        fn wait(&self) {
            let mut g = self.0 .0.lock().unwrap();
            while !*g {
                g = self.0 .1.wait(g).unwrap();
            }
        }
    }

    struct Rig {
        clock: Arc<AtomicU64>,
        loads: Arc<Mutex<Vec<String>>>,
        gates: Arc<Mutex<HashMap<String, Gate>>>,
        failing: Arc<Mutex<Vec<String>>>,
    }

    impl Rig {
        fn new() -> Self {
            Rig {
                clock: Default::default(),
                loads: Default::default(),
                gates: Default::default(),
                failing: Default::default(),
            }
        }
        fn hold(&self, id: &str) -> Gate {
            let g = Gate::default();
            self.gates.lock().unwrap().insert(id.into(), g.clone());
            g
        }
        fn loader(&self) -> Loader {
            let (loads, gates, failing) =
                (self.loads.clone(), self.gates.clone(), self.failing.clone());
            Arc::new(move |id| {
                loads.lock().unwrap().push(id.to_string());
                let gate = gates.lock().unwrap().get(id).cloned();
                if let Some(g) = gate {
                    g.wait();
                }
                if failing.lock().unwrap().iter().any(|f| f == id) {
                    return Err("corrupt".into());
                }
                Ok(Box::new(Named(id.to_string())) as Model)
            })
        }
        fn residency(&self, id: &str, memory: ModelMemory) -> Residency {
            let c = self.clock.clone();
            Residency::start(
                self.loader(),
                Arc::new(move || c.load(Ordering::SeqCst)),
                id,
                memory,
            )
        }
        fn advance(&self, ms: u64) {
            self.clock.fetch_add(ms, Ordering::SeqCst);
        }
        fn load_count(&self) -> usize {
            self.loads.lock().unwrap().len()
        }
    }

    fn eventually(mut f: impl FnMut() -> bool) {
        let t = Instant::now();
        while !f() {
            assert!(
                t.elapsed() < Duration::from_secs(5),
                "condition not reached"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn said(r: &Residency) -> String {
        r.recognizer().recognize(&[0.0], "en", None)[0].text.clone()
    }

    #[test]
    fn a_dictation_before_the_load_finishes_waits_for_it_then_is_served() {
        let rig = Rig::new();
        let gate = rig.hold("base");
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        r.dictation_started();
        let done = Arc::new(Mutex::new(None));
        let (d, r2) = (done.clone(), r.clone());
        let t = std::thread::spawn(move || *d.lock().unwrap() = Some(said(&r2)));
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            done.lock().unwrap().is_none(),
            "must wait, not skip, while the model loads"
        );
        gate.open();
        t.join().unwrap();
        assert_eq!(done.lock().unwrap().as_deref(), Some("base"));
    }

    #[test]
    fn keep_loaded_never_unloads() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        eventually(|| r.loaded_model().is_some());
        rig.advance(24 * 3600 * 1000);
        r.tick();
        assert_eq!(r.loaded_model().as_deref(), Some("base"));
    }

    #[test]
    fn unload_when_idle_frees_the_model_ten_minutes_after_the_last_dictation() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::UnloadWhenIdle);
        eventually(|| r.loaded_model().is_some());
        r.dictation_started();
        rig.advance(30 * 60 * 1000); // a long Dictation is never interrupted
        r.tick();
        assert!(r.loaded_model().is_some());
        r.dictation_ended();
        rig.advance(IDLE_UNLOAD_MS - 1);
        r.tick();
        assert!(r.loaded_model().is_some());
        rig.advance(1);
        r.tick();
        assert!(r.loaded_model().is_none());
    }

    #[test]
    fn the_next_dictation_reloads_in_parallel_without_waiting_for_the_first_decode() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::UnloadWhenIdle);
        eventually(|| r.loaded_model().is_some());
        rig.advance(IDLE_UNLOAD_MS);
        r.tick();
        assert!(r.loaded_model().is_none());
        r.dictation_started();
        eventually(|| r.loaded_model().is_some()); // loaded with no decode requested
        assert_eq!(rig.load_count(), 2);
        assert_eq!(said(&r), "base");
    }

    #[test]
    fn the_unload_deadline_is_known_only_when_one_is_pending() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::UnloadWhenIdle);
        eventually(|| r.loaded_model().is_some());
        assert_eq!(r.idle_unload_in(), Some(IDLE_UNLOAD_MS));
        rig.advance(1000);
        assert_eq!(r.idle_unload_in(), Some(IDLE_UNLOAD_MS - 1000));
        r.dictation_started();
        assert_eq!(r.idle_unload_in(), None, "never during a Dictation");
        r.dictation_ended();
        assert_eq!(r.idle_unload_in(), Some(IDLE_UNLOAD_MS));
        r.set_memory(ModelMemory::KeepLoaded);
        assert_eq!(r.idle_unload_in(), None);
        r.set_memory(ModelMemory::UnloadWhenIdle);
        rig.advance(IDLE_UNLOAD_MS);
        r.tick();
        assert_eq!(r.idle_unload_in(), None, "already unloaded");
    }

    #[test]
    fn the_waker_is_called_when_a_load_finishes() {
        let rig = Rig::new();
        let gate = rig.hold("base");
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        let woken = Arc::new(AtomicU64::new(0));
        let w = woken.clone();
        r.set_waker(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));
        gate.open();
        eventually(|| woken.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn switching_the_memory_mode_applies_at_once() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        eventually(|| r.loaded_model().is_some());
        rig.advance(IDLE_UNLOAD_MS);
        r.set_memory(ModelMemory::UnloadWhenIdle);
        r.tick();
        assert!(r.loaded_model().is_none());
        r.set_memory(ModelMemory::KeepLoaded);
        eventually(|| r.loaded_model().is_some());
    }

    #[test]
    fn swapping_keeps_the_old_model_serving_until_the_new_one_is_ready() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        eventually(|| r.loaded_model().is_some());
        let gate = rig.hold("small");
        r.request_model("small");
        assert_eq!(said(&r), "base");
        gate.open();
        eventually(|| !r.is_loading());
        r.tick();
        assert_eq!(said(&r), "small");
    }

    #[test]
    fn a_reload_loads_the_same_model_again_while_the_current_copy_serves() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        eventually(|| r.loaded_model().is_some());
        let gate = rig.hold("base");
        r.reload();
        eventually(|| rig.load_count() == 2);
        assert!(r.is_loading());
        assert_eq!(said(&r), "base", "the current copy keeps serving");
        gate.open();
        eventually(|| !r.is_loading());
        r.tick();
        assert_eq!(said(&r), "base");
    }

    #[test]
    fn a_reload_while_unloaded_loads_nothing() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::UnloadWhenIdle);
        eventually(|| r.loaded_model().is_some());
        rig.advance(IDLE_UNLOAD_MS + 1);
        r.tick();
        eventually(|| r.loaded_model().is_none());
        r.reload();
        assert_eq!(rig.load_count(), 1);
    }

    #[test]
    fn a_swap_never_lands_in_the_middle_of_a_dictation() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        eventually(|| r.loaded_model().is_some());
        r.dictation_started();
        r.request_model("small");
        eventually(|| !r.is_loading());
        r.tick();
        assert_eq!(said(&r), "base", "same model for the whole Dictation");
        assert_eq!(said(&r), "base");
        r.dictation_ended();
        assert_eq!(said(&r), "small");
    }

    #[test]
    fn a_failed_swap_keeps_the_old_model_and_leaves_a_notice() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        eventually(|| r.loaded_model().is_some());
        rig.failing.lock().unwrap().push("small".into());
        r.request_model("small");
        eventually(|| !r.is_loading());
        r.tick();
        assert_eq!(said(&r), "base");
        let notices = r.take_notices();
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("small") && notices[0].contains("keeping \"base\""));
        assert!(r.take_notices().is_empty());
    }

    #[test]
    fn a_model_chosen_while_unloaded_is_the_one_loaded_next_dictation() {
        let rig = Rig::new();
        let r = rig.residency("base", ModelMemory::UnloadWhenIdle);
        eventually(|| r.loaded_model().is_some());
        rig.advance(IDLE_UNLOAD_MS);
        r.tick();
        r.request_model("small");
        assert_eq!(
            rig.load_count(),
            1,
            "nothing is loaded just to swap an unloaded model"
        );
        r.dictation_started();
        assert_eq!(said(&r), "small");
    }

    #[test]
    fn a_failed_first_load_gives_empty_results_and_a_notice_and_retries_next_dictation() {
        let rig = Rig::new();
        rig.failing.lock().unwrap().push("base".into());
        let r = rig.residency("base", ModelMemory::KeepLoaded);
        eventually(|| !r.is_loading());
        assert!(r.recognizer().recognize(&[0.0], "en", None).is_empty());
        assert_eq!(r.take_notices().len(), 1);
        rig.failing.lock().unwrap().clear();
        r.dictation_started();
        assert_eq!(said(&r), "base");
    }
}
