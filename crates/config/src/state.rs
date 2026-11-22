//! Machine-generated state (GPU probe results, first-run, download bookkeeping).
use crate::file::{h, Schema};
use std::collections::BTreeMap;
use toml_edit::{value, DocumentMut, Item, Table};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GpuProbe {
    pub approved: bool,
    /// GPU speedup over CPU measured by the probe.
    pub speedup: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    /// A usable Speech model is installed.
    pub first_run_complete: bool,
    /// Probe result keyed by "<device>|<driver>".
    pub gpu_probes: BTreeMap<String, GpuProbe>,
    /// GPU disabled after a backend error during a Dictation.
    pub gpu_disabled_after_error: bool,
    /// The one-time switch to the Small model after a confirmed GPU was started.
    pub small_upgrade_started: bool,
    /// Partial download bytes per model id.
    pub downloads: BTreeMap<String, u64>,
}

fn sync_keys(t: &mut Table, keep: impl Fn(&str) -> bool) {
    let stale: Vec<String> = t
        .iter()
        .map(|(k, _)| k.to_string())
        .filter(|k| !keep(k))
        .collect();
    for k in stale {
        t.remove(&k);
    }
}

impl Schema for State {
    const VERSION: u32 = 1;

    fn migrate(_doc: &mut DocumentMut, _from: u32) {}

    fn read(doc: &DocumentMut) -> Result<Self, String> {
        let mut s = State {
            first_run_complete: h::get_bool(doc, "first_run_complete")?.unwrap_or(false),
            gpu_disabled_after_error: h::get_bool(doc, "gpu_disabled_after_error")?
                .unwrap_or(false),
            small_upgrade_started: h::get_bool(doc, "small_upgrade_started")?.unwrap_or(false),
            ..Default::default()
        };
        if let Some(t) = doc.get("gpu_probes") {
            let t = t.as_table().ok_or("`gpu_probes` must be a table")?;
            for (k, v) in t.iter() {
                let e = v.as_table().ok_or("`gpu_probes` entries must be tables")?;
                let approved = e
                    .get("approved")
                    .and_then(Item::as_bool)
                    .ok_or("probe `approved` must be a bool")?;
                let speedup = e
                    .get("speedup")
                    .and_then(|i| i.as_float().or_else(|| i.as_integer().map(|n| n as f64)))
                    .ok_or("probe `speedup` must be a number")?;
                s.gpu_probes
                    .insert(k.to_string(), GpuProbe { approved, speedup });
            }
        }
        if let Some(t) = doc.get("downloads") {
            let t = t.as_table().ok_or("`downloads` must be a table")?;
            for (k, v) in t.iter() {
                let n = v
                    .as_integer()
                    .filter(|n| *n >= 0)
                    .ok_or("download bytes must be a non-negative integer")?;
                s.downloads.insert(k.to_string(), n as u64);
            }
        }
        Ok(s)
    }

    fn write(&self, doc: &mut DocumentMut) {
        h::set_bool(doc, "first_run_complete", self.first_run_complete);
        h::set_bool(
            doc,
            "gpu_disabled_after_error",
            self.gpu_disabled_after_error,
        );
        h::set_bool(doc, "small_upgrade_started", self.small_upgrade_started);
        let t = h::table(doc, "gpu_probes");
        sync_keys(t, |k| self.gpu_probes.contains_key(k));
        for (k, p) in &self.gpu_probes {
            if !t.get(k).map(Item::is_table).unwrap_or(false) {
                t.insert(k, Item::Table(Table::new()));
            }
            let e = t[k.as_str()].as_table_mut().unwrap();
            e.insert("approved", value(p.approved));
            e.insert("speedup", value(p.speedup));
        }
        let t = h::table(doc, "downloads");
        sync_keys(t, |k| self.downloads.contains_key(k));
        for (k, n) in &self.downloads {
            t.insert(k, value(*n as i64));
        }
    }
}
