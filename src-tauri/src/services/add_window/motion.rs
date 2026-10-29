//! macOS motion of the add window.
//!
//! The window keeps its system frame and traffic lights, so instead of a drawn
//! panel the window itself fades in and out, in place. A native window cannot
//! scale, so where the page's panel also grows from slightly smaller, the
//! window only fades. AppKit's own show and hide animation is off, and Core
//! Animation runs the fade through `NSAnimationContext` and the window's
//! animator, the way Apple's animation guide fades windows. Durations and
//! curves match `--add-window-*` in `tokens.css`.
//!
//! With no margin around its content, the window also follows the content's
//! height frame by frame ([`fit`]).

use std::cell::Cell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSWindow, NSWindowAnimationBehavior,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::CAMediaTimingFunction;
use tauri::WebviewWindow;

/// `--add-window-fade`, the fade in, in seconds.
const FADE_IN: f64 = 0.18;
/// `--add-window-leave`, in seconds.
const LEAVE: f64 = 0.12;

/// A window ready to fade in once shown.
pub(super) struct Entrance {
    window: Retained<NSWindow>,
}

/// Prepares a hidden window for showing. With `animate` it waits transparent
/// for [`Entrance::run`]; without, it shows as it is.
/// Main thread only, like everything touching the `NSWindow`.
pub(super) fn stage(window: &WebviewWindow, animate: bool) -> Option<Entrance> {
    let window = ns_window(window)?;
    window.setAnimationBehavior(NSWindowAnimationBehavior::None);
    window.setAlphaValue(if animate { 0.0 } else { 1.0 });
    animate.then_some(Entrance { window })
}

impl Entrance {
    /// Fades the shown window in.
    pub(super) fn run(self) {
        let window = self.window;
        NSAnimationContext::runAnimationGroup(&RcBlock::new(
            move |context: NonNull<NSAnimationContext>| {
                // SAFETY: AppKit passes the current context for this group.
                let context = unsafe { context.as_ref() };
                context.setDuration(FADE_IN);
                context.setTimingFunction(Some(&curve([0.2, 0.0, 0.0, 1.0])));
                window.animator().setAlphaValue(1.0);
            },
        ));
    }
}

/// Fades the shown window out, then calls `done`; at once when there is
/// nothing to animate. The window stays ordered in, transparent, until the
/// caller hides it. Main thread only.
pub(super) fn leave(window: &WebviewWindow, done: impl FnOnce() + 'static) {
    let Some(window) = ns_window(window).filter(|window| window.isVisible()) else {
        done();
        return;
    };
    let done = Cell::new(Some(done));
    NSAnimationContext::runAnimationGroup_completionHandler(
        &RcBlock::new(move |context: NonNull<NSAnimationContext>| {
            // SAFETY: AppKit passes the current context for this group.
            let context = unsafe { context.as_ref() };
            context.setDuration(LEAVE);
            context.setTimingFunction(Some(&curve([0.4, 0.0, 1.0, 1.0])));
            window.animator().setAlphaValue(0.0);
        }),
        Some(&RcBlock::new(move || {
            if let Some(done) = done.take() {
                done();
            }
        })),
    );
}

/// Gives the window's content `width` × `height` points, keeping its top
/// edge. Main thread only.
pub(super) fn fit(window: &WebviewWindow, width: f64, height: f64) {
    let Some(window) = ns_window(window) else {
        return;
    };
    let frame = window.frame();
    let size = window
        .frameRectForContentRect(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(width, height),
        ))
        .size;
    if size == frame.size {
        return;
    }
    let top = frame.origin.y + frame.size.height;
    window.setFrame_display(
        NSRect::new(NSPoint::new(frame.origin.x, top - size.height), size),
        true,
    );
}

fn ns_window(window: &WebviewWindow) -> Option<Retained<NSWindow>> {
    MainThreadMarker::new()?;
    let pointer = window.ns_window().ok()?.cast::<NSWindow>();
    // SAFETY: Tauri hands out the live NSWindow of this window, and the
    // marker above confirms the main thread, where AppKit may be used.
    unsafe { Retained::retain(pointer) }
}

fn curve([x1, y1, x2, y2]: [f32; 4]) -> Retained<CAMediaTimingFunction> {
    CAMediaTimingFunction::functionWithControlPoints(x1, y1, x2, y2)
}
