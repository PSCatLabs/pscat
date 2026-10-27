//! The live rendering window.
//!
//! Single-threaded and step-driven, as planned in `docs/dev/ARCHITECTURE.md`: the
//! winit event loop owns the interpreter, runs a budget of interpreter
//! steps per frame, then blits the canvas. No locks, no channels, and
//! pausing/slowing execution is just a smaller budget.

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::repl::LineBuffer;
use crate::spool::Watcher;
use crate::{Interp, Limits};

const FRAME: Duration = Duration::from_millis(16);
/// How often spool mode looks at its directory while idle. Also the
/// spacing between the two sightings a new file must survive, so it
/// doubles as the settling time for files still being copied in.
const POLL: Duration = Duration::from_millis(400);

pub struct WindowOptions {
    pub title: String,
    /// Interpreter steps (executed objects) per frame — the "watchability"
    /// knob. ~60 frames/second, so 100 steps/frame ≈ 6000 objects/second.
    pub steps_per_frame: usize,
    /// Screen the canvas like a mono laser printer (`--halftone`).
    pub halftone: bool,
}

/// Cross-thread messages into the event loop. Only `--interactive`
/// sends any; the other modes share the type so one `App` serves all.
enum UserEvent {
    /// One raw line typed on stdin (reader thread → loop).
    Line(String),
    /// stdin closed (Ctrl-D) or became unreadable: end the session.
    Eof,
}

/// Run `interp` (which must already have a program queued via
/// `begin_source`) inside a live window. Returns after the window closes;
/// `Err` carries a message suitable for stderr.
pub fn run_windowed(interp: Interp, options: WindowOptions) -> Result<(), String> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| format!("cannot create event loop: {e}"))?;
    let mut app = App {
        interp,
        options,
        view: None,
        running: true,
        had_error: false,
        back: 0,
        spool: None,
        interactive: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop error: {e}"))?;
    // Files the program left open go to disk before the process exits
    // (issue #762).
    let flushed = app.interp.flush_outputs();
    if app.had_error {
        return Err("program stopped with an error (canvas left on screen)".to_string());
    }
    flushed.map_err(|e| format!("could not write a file the program left open: {e}"))
}

/// The `--interactive` mode: the terminal REPL and the live window at
/// once — type PostScript, watch it draw. Per the docs/dev/ARCHITECTURE.md
/// threading rule the interpreter stays owned by the event loop; a
/// reader thread ships raw stdin lines in through the loop's proxy and
/// never touches interpreter state. Each complete chunk runs on the
/// normal frame budget, so a long-running paste still draws live.
/// Errors are reported and the session continues, REPL-style; `quit`,
/// Ctrl-D, or closing the window ends it.
pub fn run_interactive(
    mut interp: Interp,
    options: WindowOptions,
    prelude: Option<Vec<u8>>,
) -> Result<(), String> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| format!("cannot create event loop: {e}"))?;
    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        loop {
            line.clear();
            match stdin.lock().read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let _ = proxy.send_event(UserEvent::Eof);
                    return;
                }
                Ok(_) => {
                    if proxy.send_event(UserEvent::Line(line.clone())).is_err() {
                        return; // loop is gone; session over
                    }
                }
            }
        }
    });
    // A file named alongside --interactive runs first (load a library,
    // then explore it); the first prompt appears when it finishes.
    let running = if let Some(source) = &prelude {
        interp.begin_source(source);
        true
    } else {
        false
    };
    println!(
        "pscat {} — interactive: type PostScript here, watch it draw in the window.",
        env!("PSCAT_VERSION")
    );
    println!("'quit', Ctrl-D, or closing the window exits.");
    let mut app = App {
        interp,
        options,
        view: None,
        running,
        had_error: false,
        back: 0,
        spool: None,
        interactive: Some(InteractiveState {
            buffer: LineBuffer::new(),
            queue: VecDeque::new(),
            prompt_pending: !running,
            eof: false,
        }),
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop error: {e}"))?;
    app.interp
        .flush_outputs()
        .map_err(|e| format!("could not write a file the program left open: {e}"))
}

struct InteractiveState {
    /// Joins typed lines into complete chunks (`...>` continuation).
    buffer: LineBuffer,
    /// Complete chunks waiting their turn; one runs at a time.
    queue: VecDeque<String>,
    /// A prompt is owed as soon as the loop next goes idle.
    prompt_pending: bool,
    /// stdin has closed: finish whatever is queued, then exit — so
    /// piped input (`echo ... | pscat -i`) runs to completion.
    eof: bool,
}

/// Spool mode: the window idles like the printer in the corner of the
/// lab, polling `watcher`'s directory, and prints each job that lands
/// there in a fresh interpreter (jobs must not poison each other).
/// Runs until the window closes; job errors go to stderr and printing
/// continues, so they don't fail the session.
pub fn run_spool(
    watcher: Watcher,
    page: (u32, u32),
    scale: f32,
    options: WindowOptions,
) -> Result<(), String> {
    run_spool_with_limits(watcher, page, scale, options, Limits::none())
}

/// [`run_spool`] with `limits` on every job (issue #701). Each job is one
/// program run in a fresh interpreter, so each gets the whole budget: the
/// step count, the clock and the heap baseline start again with the next
/// job. A job that passes a limit is reported on stderr like any other job
/// error (`limitexceeded (steps)`) and the spool goes on with the next.
/// Jobs run at the window's pace (`steps_per_frame` per frame), so a time
/// limit counts that pace, not only compute. `Err` if the limits cannot be
/// enforced (a memory limit without the
/// [`TrackingAllocator`](crate::TrackingAllocator) installed).
pub fn run_spool_with_limits(
    watcher: Watcher,
    page: (u32, u32),
    scale: f32,
    options: WindowOptions,
    limits: Limits,
) -> Result<(), String> {
    let mut interp = Interp::with_page_scaled(page.0, page.1, scale)
        .ok_or_else(|| format!("unusable page size {}x{}", page.0, page.1))?;
    // Checked once here, so a limit that cannot be enforced is an error at
    // startup rather than one skipped job after another.
    interp.set_limits(limits).map_err(|e| e.to_string())?;
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| format!("cannot create event loop: {e}"))?;
    let base_title = options.title.clone();
    let mut app = App {
        interp,
        options,
        view: None,
        running: false,
        had_error: false,
        back: 0,
        spool: Some(SpoolState {
            watcher,
            page,
            scale,
            limits,
            base_title,
        }),
        interactive: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("event loop error: {e}"))?;
    Ok(())
}

/// Put a spool job on `slot`: a fresh interpreter with `limits`, the job's
/// source queued. The previous job's interpreter is freed *before* the
/// limits are set, so a memory limit counts what this job grows, not what
/// the last one left behind.
fn start_job(
    slot: &mut Interp,
    page: (u32, u32),
    scale: f32,
    limits: Limits,
    source: &[u8],
) -> Result<(), String> {
    let fresh = Interp::with_page_scaled(page.0, page.1, scale)
        .ok_or_else(|| format!("unusable page size {}x{}", page.0, page.1))?;
    // Dropping the previous job's Interp frees its dictionaries
    // (`Drop for Interp`); --spool keeps constructing fresh ones for the
    // life of the process.
    drop(std::mem::replace(slot, fresh));
    slot.set_limits(limits).map_err(|e| e.to_string())?;
    slot.begin_source(source);
    Ok(())
}

struct SpoolState {
    watcher: Watcher,
    page: (u32, u32),
    scale: f32,
    /// `--max-steps`/`--max-time`/`--max-memory`: each job's own budget.
    limits: Limits,
    /// Title while idle; jobs append their file name to it.
    base_title: String,
}

struct View {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
}

struct App {
    interp: Interp,
    options: WindowOptions,
    view: Option<View>,
    running: bool,
    had_error: bool,
    /// Page navigation (arrow keys): how many pages *back* from the
    /// live canvas is being viewed. 0 = the live canvas itself.
    back: usize,
    /// Present in spool mode; the interpreter is replaced per job.
    spool: Option<SpoolState>,
    /// Present in `--interactive` mode; the interpreter persists across
    /// typed chunks (state accumulates, REPL-style).
    interactive: Option<InteractiveState>,
}

impl App {
    fn tick(&mut self) {
        if !self.running {
            return;
        }
        match self.interp.step_n(self.options.steps_per_frame) {
            Ok(true) => {}
            Ok(false) => {
                // Interactive sessions don't retitle per chunk — the
                // prompt is the "done" signal.
                if self.interactive.is_some() {
                    self.running = false;
                } else {
                    self.finish(" — done");
                }
            }
            Err(e) => {
                eprintln!("{}", self.interp.error_report(&e));
                if self.interactive.is_some() {
                    // REPL semantics: report and keep the session.
                    self.running = false;
                } else {
                    self.had_error = true;
                    self.finish(" — error (see terminal)");
                }
            }
        }
    }

    fn finish(&mut self, suffix: &str) {
        self.running = false;
        if let Some(view) = &self.view {
            view.window
                .set_title(&format!("{}{}", self.options.title, suffix));
        }
    }

    /// Interactive idle work: end the session if `quit` ran, start the
    /// next queued chunk, or — with nothing left to do — print the
    /// prompt the reader is waiting at. Call whenever the interpreter
    /// might have just gone idle.
    fn advance_interactive(&mut self, event_loop: &ActiveEventLoop) {
        if self.running || self.interactive.is_none() {
            return;
        }
        if self.interp.quit_requested() {
            event_loop.exit();
            return;
        }
        let depth = self.interp.operand_stack().len();
        let Some(state) = &mut self.interactive else {
            return;
        };
        if let Some(chunk) = state.queue.pop_front() {
            state.prompt_pending = true;
            self.interp.begin_source(chunk.as_bytes());
            self.running = true;
            self.back = 0;
            if let Some(view) = &self.view {
                view.window.request_redraw();
            }
            return;
        }
        if state.eof {
            println!();
            event_loop.exit();
            return;
        }
        if state.prompt_pending {
            state.prompt_pending = false;
            if state.buffer.is_mid_input() {
                print!("...> ");
            } else if depth == 0 {
                print!("PS> ");
            } else {
                print!("PS<{depth}> ");
            }
            let _ = std::io::stdout().flush();
        }
    }

    /// Idle-time spool work: check the directory, start the next job.
    /// One job at a time — anything else queued waits its turn.
    fn poll_spool(&mut self) {
        if self.running {
            return;
        }
        let (page, scale, limits, base_title) = {
            let Some(spool) = &mut self.spool else { return };
            if let Err(e) = spool.watcher.poll() {
                eprintln!(
                    "pscat: spool: cannot read {}: {e}",
                    spool.watcher.dir().display()
                );
                return;
            }
            (
                spool.page,
                spool.scale,
                spool.limits,
                spool.base_title.clone(),
            )
        };
        loop {
            let Some(job) = self.spool.as_mut().and_then(|s| s.watcher.next_job()) else {
                return;
            };
            let source = match std::fs::read(&job) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("pscat: spool: cannot read {}: {e}", job.display());
                    continue; // skip this job, try the next
                }
            };
            if let Err(e) = start_job(&mut self.interp, page, scale, limits, &source) {
                eprintln!("pscat: spool: cannot run {}: {e}", job.display());
                continue; // skip this job, try the next
            }
            let name = job
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| job.display().to_string());
            self.options.title = format!("{base_title} — {name}");
            self.running = true;
            self.had_error = false;
            self.back = 0;
            if let Some(view) = &self.view {
                view.window.set_title(&self.options.title);
                view.window.request_redraw();
            }
            return;
        }
    }

    fn blit(&mut self) {
        let Some(view) = &mut self.view else { return };
        let size = view.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return; // minimized
        };
        if view.surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = view.surface.buffer_mut() else {
            return;
        };

        let gfx = self.interp.gfx();
        let pixmap = if self.back > 0 {
            let pages = gfx.pages();
            &pages[pages.len() - self.back]
        } else {
            &gfx.pixmap
        };
        let screened;
        let pixmap = if self.options.halftone {
            screened = crate::halftone::screen(pixmap);
            &screened
        } else {
            pixmap
        };
        let (pw, ph) = (pixmap.width() as usize, pixmap.height() as usize);
        let data = pixmap.data();
        let (bw, bh) = (size.width as usize, size.height as usize);
        // Nearest-neighbor scale from the page pixmap to the physical
        // window buffer (handles HiDPI's 2x factor adequately for now).
        for dy in 0..bh {
            let sy = (dy * ph / bh).min(ph - 1);
            let row = &data[sy * pw * 4..(sy + 1) * pw * 4];
            let out = &mut buffer[dy * bw..(dy + 1) * bw];
            for (dx, px) in out.iter_mut().enumerate() {
                let sx = (dx * pw / bw).min(pw - 1);
                let p = &row[sx * 4..sx * 4 + 4];
                *px = u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]);
            }
        }
        let _ = buffer.present();
        self.interp.gfx_mut().dirty = false;
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.view.is_some() {
            return;
        }
        let (pw, ph) = {
            let pm = &self.interp.gfx().pixmap;
            (pm.width(), pm.height())
        };
        let attrs = Window::default_attributes()
            .with_title(&self.options.title)
            .with_inner_size(LogicalSize::new(pw, ph));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("pscat: cannot create window: {e}");
                event_loop.exit();
                return;
            }
        };
        let surface =
            Context::new(window.clone()).and_then(|ctx| Surface::new(&ctx, window.clone()));
        match surface {
            Ok(surface) => {
                window.request_redraw();
                self.view = Some(View { window, surface });
            }
            Err(e) => {
                eprintln!("pscat: cannot create render surface: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                self.tick();
                self.blit();
                self.advance_interactive(event_loop);
                if self.running {
                    // ~60fps cadence while the program runs; idle after.
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + FRAME));
                } else if self.spool.is_some() {
                    // Spool mode never truly idles: keep the poll
                    // timer armed for the next job to land.
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL));
                } else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
            WindowEvent::Resized(_) => {
                if let Some(view) = &self.view {
                    view.window.request_redraw();
                }
            }
            // Left/Right browse completed pages; the live canvas is
            // the newest "page".
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(key @ (NamedKey::ArrowLeft | NamedKey::ArrowRight)),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                let npages = self.interp.gfx().pages().len();
                self.back = match key {
                    NamedKey::ArrowLeft => (self.back + 1).min(npages),
                    _ => self.back.saturating_sub(1),
                };
                if let Some(view) = &self.view {
                    let total = npages + 1;
                    let showing = total - self.back;
                    view.window
                        .set_title(&format!("{} — page {showing}/{total}", self.options.title));
                    view.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Eof => {
                if let Some(state) = &mut self.interactive {
                    state.eof = true;
                    self.advance_interactive(event_loop);
                } else {
                    event_loop.exit();
                }
            }
            UserEvent::Line(line) => {
                let Some(state) = &mut self.interactive else {
                    return;
                };
                if let Some(chunk) = state.buffer.push_line(&line) {
                    state.queue.push_back(chunk);
                }
                // Whatever happens next — chunk runs, or we're
                // mid-procedure — the typist is owed a fresh prompt.
                state.prompt_pending = true;
                self.advance_interactive(event_loop);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.running {
            if let Some(view) = &self.view {
                view.window.request_redraw();
            }
            return;
        }
        if self.spool.is_some() {
            self.poll_spool();
            if self.running {
                if let Some(view) = &self.view {
                    view.window.request_redraw();
                }
            } else {
                // Re-arm the timer each pass: a stored WaitUntil
                // instant in the past would spin the loop.
                event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PsError;

    /// Step `interp` until its program ends or errors, at most `cap` frames.
    fn run_out(interp: &mut Interp) -> Result<(), PsError> {
        for _ in 0..100_000 {
            if !interp.step_n(1000)? {
                return Ok(());
            }
        }
        panic!("the job neither ended nor hit a limit");
    }

    #[test]
    fn a_spool_job_runs_under_the_limits() {
        let limits = Limits::none().with_max_steps(5_000);
        let mut slot = Interp::with_page_scaled(100, 100, 1.0).unwrap();
        start_job(&mut slot, (100, 100), 1.0, limits, b"{ } loop").unwrap();
        assert_eq!(run_out(&mut slot), Err(PsError::StepsExceeded));
        // The next job starts again from zero on a fresh interpreter, so one
        // that fits is not charged for the runaway before it.
        start_job(&mut slot, (100, 100), 1.0, limits, b"1 1 add pop").unwrap();
        assert_eq!(run_out(&mut slot), Ok(()));
        // And with no limits, the old behaviour: nothing stops a job early.
        start_job(
            &mut slot,
            (100, 100),
            1.0,
            Limits::none(),
            b"0 1 20000 { pop } for",
        )
        .unwrap();
        assert_eq!(run_out(&mut slot), Ok(()));
    }

    #[test]
    fn the_spool_hands_its_limits_to_each_job_it_picks_up() {
        let dir = std::env::temp_dir().join(format!("pscat-spool-limits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let watcher = Watcher::new(&dir).unwrap();
        let limits = Limits::none().with_max_steps(5_000);
        let mut app = App {
            interp: Interp::with_page_scaled(100, 100, 1.0).unwrap(),
            options: WindowOptions {
                title: "t".into(),
                steps_per_frame: 1000,
                halftone: false,
            },
            view: None,
            running: false,
            had_error: false,
            back: 0,
            spool: Some(SpoolState {
                watcher,
                page: (100, 100),
                scale: 1.0,
                limits,
                base_title: "t".into(),
            }),
            interactive: None,
        };
        std::fs::write(dir.join("loop.ps"), "{ } loop").unwrap();
        // A new file must hold still across two polls before it is a job.
        app.poll_spool();
        assert!(!app.running);
        app.poll_spool();
        assert!(app.running, "the job should have started");
        for _ in 0..100_000 {
            if !app.running {
                break;
            }
            app.tick();
        }
        assert!(!app.running, "the runaway job was never stopped");
        assert!(app.had_error, "stopping at the limit is a job error");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
