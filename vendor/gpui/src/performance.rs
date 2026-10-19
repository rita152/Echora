//! Opt-in, content-free diagnostics. Disabled runs only check a OnceLock.
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    panic::Location,
    sync::{Mutex, OnceLock},
    time::Instant,
};
struct Recorder {
    output: BufWriter<File>,
    start: Instant,
    events: BTreeMap<String, u64>,
    input: Option<Instant>,
}
fn recorder() -> Option<&'static Mutex<Recorder>> {
    static RECORDER: OnceLock<Option<Mutex<Recorder>>> = OnceLock::new();
    RECORDER
        .get_or_init(|| {
            let path = std::env::var_os("ECHORA_PERF_LOG")?;
            let output = BufWriter::new(File::create(path).ok()?);
            Some(Mutex::new(Recorder {
                output,
                start: Instant::now(),
                events: BTreeMap::new(),
                input: None,
            }))
        })
        .as_ref()
}
/// Start an optional timing interval.
pub fn start() -> Option<Instant> {
    recorder().map(|_| Instant::now())
}
/// Count an invalidation or rebuild without recording user content.
#[track_caller]
pub fn event(kind: &str) {
    if let Some(recorder) = recorder() {
        let source = Location::caller();
        let mut r = recorder.lock().unwrap();
        *r.events
            .entry(format!("{kind}@{}:{}", source.file(), source.line()))
            .or_default() += 1;
    }
}
/// Record a timed phase. CPU submission is not GPU presentation time.
pub fn end(kind: &str, start: Option<Instant>) {
    if let (Some(start), Some(recorder)) = (start, recorder()) {
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        let mut r = recorder.lock().unwrap();
        let row = json!({"phase":kind,"t_ms":r.start.elapsed().as_secs_f64()*1000.,"duration_ms":elapsed});
        let _ = writeln!(r.output, "{row}");
    }
}
/// Finish a frame, including invalidation sources observed since the last frame.
pub fn frame(start: Option<Instant>, width: f32, height: f32, dpr: f32) {
    if let (Some(start), Some(recorder)) = (start, recorder()) {
        let mut r = recorder.lock().unwrap();
        let events = std::mem::take(&mut r.events);
        let input_to_draw_end_ms = r.input.take().map(|t| t.elapsed().as_secs_f64() * 1000.);
        let row = json!({"phase":"frame","t_ms":r.start.elapsed().as_secs_f64()*1000.,"duration_ms":start.elapsed().as_secs_f64()*1000.,"input_to_draw_end_ms":input_to_draw_end_ms,"width":width,"height":height,"dpr":dpr,"events":events});
        let _ = writeln!(r.output, "{row}");
        let _ = r.output.flush();
    }
}

/// Timestamp a scroll event; measures delivery to CPU draw completion, not display latency.
pub fn scroll_input() {
    if let Some(recorder) = recorder() {
        recorder
            .lock()
            .unwrap()
            .input
            .get_or_insert_with(Instant::now);
    }
}
