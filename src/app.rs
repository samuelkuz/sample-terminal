use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, OnceLock};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApp, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent, NSFont,
    NSFontAttributeName, NSForegroundColorAttributeName, NSRectFill, NSResponder, NSStringDrawing,
    NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    NSDictionary, NSAttributedStringKey, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
    NSTimer,
};

use crate::session::TerminalSession;
use crate::terminal_buffer::TerminalBuffer;

const WINDOW_WIDTH: f64 = 900.0;
const WINDOW_HEIGHT: f64 = 640.0;
const FONT_SIZE: f64 = 14.0;
const LINE_HEIGHT: f64 = 18.0;
const H_PADDING: f64 = 12.0;
const V_PADDING: f64 = 12.0;

static APP_STATE: OnceLock<Arc<AppState>> = OnceLock::new();

pub fn run() -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("failed to acquire main thread marker")?;
    let state = Arc::new(AppState::new()?);
    APP_STATE
        .set(Arc::clone(&state))
        .map_err(|_| "application state already initialized".to_string())?;

    let app = NSApp(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let window = create_window(mtm);
    let view = TerminalView::new(
        mtm,
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
    );
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

struct AppState {
    session: TerminalSession,
    buffer: Mutex<TerminalBuffer>,
}

impl AppState {
    fn new() -> Result<Self, String> {
        Ok(Self {
            session: TerminalSession::spawn()?,
            buffer: Mutex::new(TerminalBuffer::new()),
        })
    }

    fn poll_session(&self) -> bool {
        let chunks = self.session.try_read();
        if chunks.is_empty() {
            return false;
        }

        let Ok(mut buffer) = self.buffer.lock() else {
            return false;
        };

        for chunk in chunks {
            buffer.push_bytes(&chunk);
        }

        true
    }

    fn visible_lines(&self, max_lines: usize) -> Vec<String> {
        let Ok(buffer) = self.buffer.lock() else {
            return Vec::new();
        };

        buffer.visible_lines(max_lines)
    }

    fn send_input(&self, bytes: &[u8]) {
        self.session.write_input(bytes);
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
    #[ivars = ()]
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

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                key_down_impl(event);
            }));
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, dirty_rect: NSRect) {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                draw_rect_impl(self, dirty_rect);
            }));
        }
    }
);

impl TerminalView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), initWithFrame: frame] }
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
    if !text.is_empty() {
        state.send_input(text.as_bytes());
    }
}

fn draw_rect_impl(this: &TerminalView, dirty_rect: NSRect) {
    unsafe {
        let Some(state) = app_state() else {
            return;
        };

        let background = NSColor::colorWithCalibratedRed_green_blue_alpha(0.08, 0.09, 0.11, 1.0);
        let foreground = NSColor::colorWithCalibratedRed_green_blue_alpha(0.91, 0.93, 0.95, 1.0);
        background.setFill();
        NSRectFill(dirty_rect);

        let bounds = this.bounds();
        let visible_line_count =
            ((bounds.size.height - (V_PADDING * 2.0)) / LINE_HEIGHT).floor().max(1.0) as usize;
        let lines = state.visible_lines(visible_line_count);

        let font_name = NSString::from_str("Menlo");
        let font = if let Some(font) = NSFont::fontWithName_size(&font_name, FONT_SIZE) {
            font
        } else {
            NSFont::userFixedPitchFontOfSize(FONT_SIZE)
                .unwrap_or_else(|| NSFont::systemFontOfSize(FONT_SIZE))
        };

        let keys = [NSFontAttributeName, NSForegroundColorAttributeName];
        let values = [
            Retained::as_ptr(&font).cast::<AnyObject>(),
            Retained::as_ptr(&foreground).cast::<AnyObject>(),
        ];
        let attributes: Retained<NSDictionary<NSAttributedStringKey, AnyObject>> = msg_send![
            NSDictionary::<NSAttributedStringKey, AnyObject>::class(),
            dictionaryWithObjects: values.as_ptr(),
            forKeys: keys.as_ptr(),
            count: keys.len(),
        ];

        for (index, line) in lines.iter().enumerate() {
            let point = NSPoint::new(H_PADDING, V_PADDING + (index as f64 * LINE_HEIGHT));
            let ns_string = NSString::from_str(line);
            ns_string.drawAtPoint_withAttributes(point, Some(&attributes));
        }
    }
}

fn tick_impl() {
    let Some(state) = app_state() else {
        return;
    };

    if state.poll_session() {
        let mtm = MainThreadMarker::new().expect("tick runs on the main thread");
        let app = NSApp(mtm);
        if let Some(window) = app.keyWindow() {
            if let Some(view) = window.contentView() {
                view.setNeedsDisplay(true);
            }
        }
    }
}
