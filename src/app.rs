use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, OnceLock};

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApp, NSApplicationActivationPolicy, NSBackingStoreType, NSEvent, NSResponder, NSView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer};

use crate::app_state::AppState;
use crate::input::{SelectionPhase, normalized_scroll_lines, terminal_input_bytes};
use crate::layout::{point_to_cell, terminal_grid_size};
use crate::renderer::{RenderFrameInput, TerminalRenderer};

const WINDOW_WIDTH: f64 = 900.0;
const WINDOW_HEIGHT: f64 = 640.0;

static APP_STATE: OnceLock<Arc<AppState>> = OnceLock::new();

pub fn run() -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("failed to acquire main thread marker")?;
    let (cols, rows) = terminal_grid_size(WINDOW_WIDTH, WINDOW_HEIGHT);
    let state = Arc::new(AppState::new(cols, rows)?);
    APP_STATE
        .set(Arc::clone(&state))
        .map_err(|_| "application state already initialized".to_string())?;

    let app = NSApp(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let window = create_window(mtm);
    let view = TerminalView::new(
        mtm,
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
        ),
    )?;
    window.setContentView(Some(&view));
    let responder: &NSResponder = &view;
    window.makeFirstResponder(Some(responder));
    window.makeKeyAndOrderFront(None);

    let timer_target = TimerTarget::new(mtm);
    unsafe {
        let _timer = NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            1.0 / 60.0,
            &*timer_target,
            sel!(tick:),
            None,
            true,
        );
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        app.run();
    }

    Ok(())
}

struct TerminalViewState {
    renderer: Option<TerminalRenderer>,
    startup_error: Option<String>,
}

impl TerminalViewState {
    fn new() -> Self {
        Self {
            renderer: None,
            startup_error: None,
        }
    }
}

fn create_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
    );
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;

    unsafe {
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        window.setReleasedWhenClosed(false);
        window.cascadeTopLeftFromPoint(NSPoint::new(20.0, 20.0));
        window.center();
        window.setTitle(&NSString::from_str("Sample Terminal"));
        window
    }
}

fn app_state() -> Option<&'static Arc<AppState>> {
    APP_STATE.get()
}

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<TerminalViewState>]
    struct TerminalView;

    unsafe impl NSObjectProtocol for TerminalView {}

    impl TerminalView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(wantsUpdateLayer))]
        fn wants_update_layer(&self) -> bool {
            true
        }

        #[unsafe(method(updateLayer))]
        fn update_layer(&self) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                render_view(self);
            }));
        }

        #[unsafe(method(renderFrame))]
        fn render_frame(&self) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                render_view(self);
            }));
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                key_down_impl(event);
            }));
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                selection_event(self, event, SelectionPhase::Start);
            }));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                selection_event(self, event, SelectionPhase::Update);
            }));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                selection_event(self, event, SelectionPhase::End);
            }));
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                scroll_event(self, event);
            }));
        }

        #[unsafe(method(viewDidChangeBackingProperties))]
        fn view_did_change_backing_properties(&self) {
            let _: () = unsafe { msg_send![super(self), viewDidChangeBackingProperties] };
            render_view(self);
        }

        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, new_size: NSSize) {
            let _: () = unsafe { msg_send![super(self), setFrameSize: new_size] };
            render_view(self);
        }
    }
);

impl TerminalView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Result<Retained<Self>, String> {
        let this = Self::alloc(mtm).set_ivars(RefCell::new(TerminalViewState::new()));
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.finish_setup()?;
        Ok(view)
    }

    fn finish_setup(&self) -> Result<(), String> {
        self.setWantsLayer(true);

        let renderer = TerminalRenderer::new()?;
        self.setLayer(Some(renderer.layer()));

        let device_name = renderer.device_name();
        if let Ok(mut state) = self.ivars().try_borrow_mut() {
            state.renderer = Some(renderer);
            state.startup_error = None;
        } else {
            return Err("terminal view state is already borrowed during setup".to_string());
        }

        render_view(self);
        let title = NSString::from_str(&format!("Sample Terminal ({device_name})"));
        if let Some(window) = self.window() {
            window.setTitle(&title);
        }

        Ok(())
    }
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    struct TimerTarget;

    unsafe impl NSObjectProtocol for TimerTarget {}

    impl TimerTarget {
        #[unsafe(method(tick:))]
        fn tick(&self, _: &NSTimer) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                tick_impl();
            }));
        }
    }
);

impl TimerTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

fn key_down_impl(event: &NSEvent) {
    let Some(state) = app_state() else {
        return;
    };

    let Some(characters) = event.characters() else {
        return;
    };

    let text = characters.to_string();
    if let Some(sequence) = terminal_input_bytes(&text) {
        state.send_input(&sequence);
    }
}

fn selection_event(view: &TerminalView, event: &NSEvent, phase: SelectionPhase) {
    let Some(app_state) = app_state() else {
        return;
    };

    let bounds = view.bounds();
    let (cols, rows) = terminal_grid_size(bounds.size.width, bounds.size.height);
    let point = view.convertPoint_fromView(event.locationInWindow(), None);
    let cell = point_to_cell(
        bounds.size.width,
        bounds.size.height,
        cols,
        rows,
        point.x,
        point.y,
    );

    app_state.update_selection(phase, cell);
    render_view(view);
}

fn scroll_event(view: &TerminalView, event: &NSEvent) {
    let Some(app_state) = app_state() else {
        return;
    };

    let lines = normalized_scroll_lines(event.scrollingDeltaY(), event.hasPreciseScrollingDeltas());
    if lines == 0 {
        return;
    }

    app_state.scroll_viewport(lines);
    app_state.stop_selection_drag();

    render_view(view);
}

fn render_view(view: &TerminalView) {
    let Some(app_state) = app_state() else {
        return;
    };

    let Ok(mut state) = view.ivars().try_borrow_mut() else {
        return;
    };
    let Some(renderer) = state.renderer.as_mut() else {
        if state.startup_error.is_none() {
            state.startup_error = Some("Metal renderer was not initialized".to_string());
        }
        return;
    };

    let bounds = view.bounds();
    let backing = view.convertRectToBacking(bounds);
    let scale_factor = view
        .window()
        .map(|window| window.backingScaleFactor())
        .unwrap_or(1.0);
    let (terminal_cols, terminal_rows) = terminal_grid_size(bounds.size.width, bounds.size.height);
    app_state.sync_window_size(
        terminal_cols,
        terminal_rows,
        backing.size.width.round().clamp(1.0, u16::MAX as f64) as u16,
        backing.size.height.round().clamp(1.0, u16::MAX as f64) as u16,
    );

    let cursor_visible = app_state.cursor_visible();
    let selection = app_state.selection_range();
    let render_state = app_state.render_snapshot(terminal_cols, terminal_rows, cursor_visible);

    let input = RenderFrameInput {
        view_width: bounds.size.width,
        view_height: bounds.size.height,
        pixel_width: backing.size.width.max(1.0),
        pixel_height: backing.size.height.max(1.0),
        scale_factor,
        cursor_visible,
        selection,
    };

    if let Err(error) = renderer.render(input, &render_state) {
        eprintln!("render error: {error}");
    }
}

fn tick_impl() {
    let Some(state) = app_state() else {
        return;
    };

    let should_render = state.poll_session_and_should_render();
    if !should_render {
        return;
    }

    let mtm = MainThreadMarker::new().expect("tick runs on the main thread");
    let app = NSApp(mtm);
    if let Some(window) = app.keyWindow() {
        if let Some(view) = window.contentView() {
            let _: () = unsafe { msg_send![&*view, renderFrame] };
        }
    }
}
