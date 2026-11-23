//! Diagnostic: lists every input device with its shared-mode format, then records a few
//! seconds from each and prints the level of every channel. Speak while it runs.
//! `cargo run -p aloud-type-platform-windows --example mic_probe`
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|d| d.id().ok())
        .map(|i| i.to_string());
    let Ok(devices) = host.input_devices() else {
        println!("no input devices");
        return;
    };
    for dev in devices {
        let id = dev.id().map(|i| i.to_string()).unwrap_or_default();
        let name = dev
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "?".into());
        let mark = if Some(&id) == default_id.as_ref() {
            " (default)"
        } else {
            ""
        };
        println!("\n== {name}{mark}\n   id {id}");
        let cfg = match dev.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                println!("   default config error: {e}");
                continue;
            }
        };
        let (rate, ch, fmt) = (cfg.sample_rate(), cfg.channels(), cfg.sample_format());
        println!("   default: {rate} Hz, {ch} ch, {fmt:?}");
        if let Ok(all) = dev.supported_input_configs() {
            for c in all {
                println!(
                    "   supports: {}-{} Hz, {} ch, {:?}",
                    c.min_sample_rate(),
                    c.max_sample_rate(),
                    c.channels(),
                    c.sample_format()
                );
            }
        }
        let levels = Arc::new(Mutex::new((
            vec![0f32; ch as usize],
            vec![0f64; ch as usize],
            0usize,
        )));
        let config = cpal::StreamConfig {
            channels: ch,
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        };
        let push = {
            let levels = levels.clone();
            move |data: &[f32]| {
                let mut g = levels.lock().unwrap();
                for frame in data.chunks_exact(ch as usize) {
                    for (i, s) in frame.iter().enumerate() {
                        g.0[i] = g.0[i].max(s.abs());
                        g.1[i] += (*s as f64) * (*s as f64);
                    }
                    g.2 += 1;
                }
            }
        };
        let on_err = |e: cpal::Error| println!("   stream error: {e}");
        let stream = match fmt {
            SampleFormat::F32 => {
                dev.build_input_stream(config, move |d: &[f32], _| push(d), on_err, None)
            }
            SampleFormat::I16 => dev.build_input_stream(
                config,
                move |d: &[i16], _| {
                    push(&d.iter().map(|s| *s as f32 / 32768.0).collect::<Vec<_>>())
                },
                on_err,
                None,
            ),
            SampleFormat::I32 => dev.build_input_stream(
                config,
                move |d: &[i32], _| {
                    push(
                        &d.iter()
                            .map(|s| *s as f32 / 2147483648.0)
                            .collect::<Vec<_>>(),
                    )
                },
                on_err,
                None,
            ),
            other => {
                println!("   format {other:?} not handled by this probe");
                continue;
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                println!("   open error: {e}");
                continue;
            }
        };
        if let Err(e) = stream.play() {
            println!("   play error: {e}");
            continue;
        }
        println!("   recording 3 s...");
        std::thread::sleep(Duration::from_secs(3));
        drop(stream);
        let g = levels.lock().unwrap();
        println!("   frames {} ({:.2} s)", g.2, g.2 as f64 / rate as f64);
        for i in 0..ch as usize {
            let rms = if g.2 > 0 {
                (g.1[i] / g.2 as f64).sqrt()
            } else {
                0.0
            };
            let db = 20.0 * rms.max(1e-9).log10();
            println!(
                "   ch{i}: peak {:.4}  rms {:.5} ({db:.1} dBFS)",
                g.0[i], rms
            );
        }
    }
}
