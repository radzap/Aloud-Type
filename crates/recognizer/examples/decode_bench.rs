//! Times Whisper decodes on this machine (CPU latency spike, issue 23).
//!
//!   cargo run --release -p aloud-type-recognizer --example decode_bench -- <model.bin> <16k-mono-16bit.wav> [threads...]
//!
//! Prints the transcript and the time of each decode for each thread count.
use aloud_type_recognizer::WhisperRecognizer;
use std::path::Path;
use std::time::Instant;

fn read_wav(path: &str) -> Vec<f32> {
    let b = std::fs::read(path).expect("read wav");
    // Find the "data" chunk; assume 16 kHz mono 16-bit PCM.
    let mut i = 12;
    while i + 8 <= b.len() {
        let len = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if &b[i..i + 4] == b"data" {
            return b[i + 8..(i + 8 + len).min(b.len())]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0)
                .collect();
        }
        i += 8 + len + (len & 1);
    }
    panic!("no data chunk");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (model, wav) = (&args[1], &args[2]);
    let threads: Vec<usize> = if args.len() > 3 {
        args[3..].iter().map(|t| t.parse().unwrap()).collect()
    } else {
        vec![aloud_type_recognizer::decode_threads(
            num_cpus::get_physical(),
        )]
    };
    let audio = read_wav(wav);
    println!("audio {:.1} s", audio.len() as f32 / 16000.0);
    for t in threads {
        let mut r = WhisperRecognizer::load_with_threads(Path::new(model), t).expect("load");
        let mut times = Vec::new();
        let mut text = String::new();
        for _ in 0..4 {
            let s = Instant::now();
            let segs = r.try_recognize(&audio, "en", None).expect("decode");
            times.push(s.elapsed().as_millis());
            text = segs
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
        }
        println!("threads {t:2}: {times:?} ms  | {text}");
    }
}
