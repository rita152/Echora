//! Opt-in, content-free diagnostics. Disabled runs only check a OnceLock.
use serde_json::json;
use std::{
    cell::Cell,
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
    presentation_input: BTreeMap<u64, Instant>,
    last_presentation: BTreeMap<u64, f64>,
}
thread_local! {
    static PRESENTATION_WINDOW: Cell<Option<u64>> = const { Cell::new(None) };
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
                presentation_input: BTreeMap::new(),
                last_presentation: BTreeMap::new(),
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
pub fn scroll_input(window: u64) {
    interaction_input(window);
}

/// The start of a measured interaction, at delivery to the UI. This excludes
/// hardware latency and time the event spent in the operating system queue.
pub fn interaction_input(window: u64) {
    if let Some(recorder) = recorder() {
        let mut r = recorder.lock().unwrap();
        let now = Instant::now();
        r.input.get_or_insert(now);
        r.presentation_input.entry(window).or_insert(now);
    }
}

/// Associates a synchronous platform draw with its GPUI window. The platform's
/// asynchronous presented callback carries this identity with the drawable.
pub fn with_presentation_window<R>(window: u64, draw: impl FnOnce() -> R) -> R {
    if recorder().is_none() {
        return draw();
    }
    PRESENTATION_WINDOW.with(|current| {
        struct Restore<'a>(&'a Cell<Option<u64>>, Option<u64>);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _restore = Restore(current, current.replace(Some(window)));
        draw()
    })
}

/// Captured before submitting a drawable. `host_time` must use the platform's
/// presentation clock (CACurrentMediaTime on macOS), in seconds.
#[derive(Clone, Copy)]
pub struct PresentationFrame {
    window: u64,
    host_time: f64,
    submitted_at_ms: f64,
    input_age_ms: Option<f64>,
    input: Option<Instant>,
}

/// Starts tracking a drawable for the current presentation window, if enabled.
pub fn presentation_frame(host_time: f64) -> Option<PresentationFrame> {
    let recorder = recorder()?;
    let window = PRESENTATION_WINDOW.with(Cell::get)?;
    let r = recorder.lock().unwrap();
    let input = r.presentation_input.get(&window).copied();
    let input_age_ms = input.map(|input| input.elapsed().as_secs_f64() * 1000.);
    Some(PresentationFrame {
        window,
        host_time,
        submitted_at_ms: r.start.elapsed().as_secs_f64() * 1000.,
        input_age_ms,
        input,
    })
}

/// Records the drawable's actual display time supplied by the platform, not
/// callback execution time or GPU completion. A zero timestamp means it was
/// not displayed and must not enter interval/latency statistics.
pub fn presented(frame: PresentationFrame, drawable: u64, host_time: f64, width: f32, height: f32) {
    let Some(recorder) = recorder() else { return };
    let mut r = recorder.lock().unwrap();
    if !host_time.is_finite() || host_time <= 0. {
        let row = json!({"phase":"presentation_unavailable","t_ms":r.start.elapsed().as_secs_f64()*1000.,"submitted_at_ms":frame.submitted_at_ms,"host_submit_time_ms":frame.host_time*1000.,"device_width":width,"device_height":height,"window_id":frame.window,"drawable_id":drawable});
        let _ = writeln!(r.output, "{row}");
        return;
    }
    let previous = r.last_presentation.get(&frame.window).copied();
    let interval_ms = previous
        .filter(|previous| *previous <= host_time)
        .map(|previous| (host_time - previous) * 1000.);
    if previous.is_none_or(|previous| previous < host_time) {
        r.last_presentation.insert(frame.window, host_time);
    }
    let submit_to_present_ms = (host_time - frame.host_time) * 1000.;
    let input_to_present_ms = if frame.input.is_some()
        && r.presentation_input.get(&frame.window) == frame.input.as_ref()
    {
        r.presentation_input.remove(&frame.window);
        frame.input_age_ms.map(|age| age + submit_to_present_ms)
    } else {
        None
    };
    let row = json!({"phase":"presentation","t_ms":r.start.elapsed().as_secs_f64()*1000.,"submitted_at_ms":frame.submitted_at_ms,"host_submit_time_ms":frame.host_time*1000.,"duration_ms":submit_to_present_ms,"presented_time_ms":host_time*1000.,"interval_ms":interval_ms,"input_delivery_to_present_ms":input_to_present_ms,"window_id":frame.window,"drawable_id":drawable,"device_width":width,"device_height":height});
    let _ = writeln!(r.output, "{row}");
    let _ = r.output.flush();
}
