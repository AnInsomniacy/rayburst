//! macOS motion of the add window.
//!
//! The window keeps its system frame and traffic lights, so instead of a drawn
//! panel the window itself moves: it rises into place as it fades in, and
//! sinks away as it fades out, as the panel does on Windows and Linux. AppKit's
//! own show and hide animation is off, and Core Animation runs the motion
//! through `NSAnimationContext` and the window's animator, the way Apple's
//! animation guide fades and moves windows. Distances, durations and curves
//! match `--add-window-*` and `--dialog-leave` in `tokens.css`; a native window
//! cannot scale, so it only rises and fades.
//!
//! With no margin around its content, the window also follows the content's
//! height frame by frame ([`fit`]). Its frame changes go through the same
//! animator: a frame set directly while the rise runs would be overwritten by
//! the rise, which would end at the height it started with.

use std::cell::Cell;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSWindow, NSWindowAnimationBehavior,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::CAMediaTimingFunction;
use tauri::WebviewWindow;

/// How far below its place the window starts, in points.
const RISE: f64 = 14.0;
/// `--add-window-enter`, the rise, in seconds.
const ENTER: f64 = 0.48;
/// `--add-window-fade`, the fade in, in seconds.
const FADE_IN: f64 = 0.3;
/// How far the window sinks as it fades out, in points.
const SINK: f64 = 6.0;
/// `--dialog-leave`, in seconds.
const LEAVE: f64 = 0.17;
const RISE_CURVE: [f32; 4] = [0.25, 0.8, 0.25, 1.0];

/// The rise in progress: the top edge it ends at, and when it ends.
#[derive(Clone, Copy)]
struct Rising {
    top: f64,
    end: Instant,
}

thread_local! {
    // Main thread only, like the window it describes.
    static RISING: Cell<Option<Rising>> = const { Cell::new(None) };
}

/// A window placed below its position, transparent, ready to rise once shown.
pub(super) struct Entrance {
    window: Retained<NSWindow>,
    frame: NSRect,
}

/// Prepares a hidden window for showing. With `animate` it waits transparent
/// below its position for [`Entrance::run`]; without, it shows as it is.
/// Main thread only, like everything touching the `NSWindow`.
pub(super) fn stage(window: &WebviewWindow, animate: bool) -> Option<Entrance> {
    let window = ns_window(window)?;
    window.setAnimationBehavior(NSWindowAnimationBehavior::None);
    if !animate {
        window.setAlphaValue(1.0);
        return None;
    }
    let frame = window.frame();
    window.setAlphaValue(0.0);
    window.setFrame_display(shifted(frame, RISE), false);
    Some(Entrance { window, frame })
}

impl Entrance {
    /// Lifts the shown window into place.
    pub(super) fn run(self) {
        let Entrance { window, frame } = self;
        RISING.set(Some(Rising {
            top: top(frame),
            end: Instant::now() + Duration::from_secs_f64(ENTER),
        }));
        let lift = window.clone();
        NSAnimationContext::runAnimationGroup(&RcBlock::new(
            move |context: NonNull<NSAnimationContext>| {
                // SAFETY: AppKit passes the current context for this group.
                let context = unsafe { context.as_ref() };
                context.setDuration(ENTER);
                context.setTimingFunction(Some(&curve(RISE_CURVE)));
                lift.animator().setFrame_display(frame, true);
            },
        ));
        NSAnimationContext::runAnimationGroup(&RcBlock::new(
            move |context: NonNull<NSAnimationContext>| {
                // SAFETY: as above.
                let context = unsafe { context.as_ref() };
                context.setDuration(FADE_IN);
                context.setTimingFunction(Some(&curve([0.2, 0.0, 0.0, 1.0])));
                window.animator().setAlphaValue(1.0);
            },
        ));
    }
}

/// Fades the shown window out as it sinks, then calls `done`; at once when
/// there is nothing to animate. The window stays ordered in, transparent,
/// until the caller hides it. Main thread only.
pub(super) fn leave(window: &WebviewWindow, done: impl FnOnce() + 'static) {
    let Some(window) = ns_window(window).filter(|window| window.isVisible()) else {
        done();
        return;
    };
    let target = shifted(window.frame(), SINK);
    let done = Cell::new(Some(done));
    NSAnimationContext::runAnimationGroup_completionHandler(
        &RcBlock::new(move |context: NonNull<NSAnimationContext>| {
            // SAFETY: AppKit passes the current context for this group.
            let context = unsafe { context.as_ref() };
            context.setDuration(LEAVE);
            context.setTimingFunction(Some(&curve([0.4, 0.0, 1.0, 1.0])));
            let animator = window.animator();
            animator.setAlphaValue(0.0);
            animator.setFrame_display(target, true);
        }),
        Some(&RcBlock::new(move || {
            if let Some(done) = done.take() {
                done();
            }
        })),
    );
}

/// Gives the window's content `width` × `height` points, keeping its top edge.
/// During the rise the window keeps rising, toward its place at the new
/// height, for the rest of the rise. Main thread only.
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
    // A rise in progress already heads for its place at this size.
    if size == frame.size {
        return;
    }
    let now = Instant::now();
    let rising = RISING.get().filter(|rising| rising.end > now);
    let edge = rising.map_or_else(|| top(frame), |rising| rising.top);
    let target = NSRect::new(NSPoint::new(frame.origin.x, edge - size.height), size);
    let remaining = rising.map_or(0.0, |rising| (rising.end - now).as_secs_f64());
    NSAnimationContext::runAnimationGroup(&RcBlock::new(
        move |context: NonNull<NSAnimationContext>| {
            // SAFETY: AppKit passes the current context for this group.
            let context = unsafe { context.as_ref() };
            // Zero still replaces a frame animation in flight (the sink).
            context.setDuration(remaining);
            context.setTimingFunction(Some(&curve(RISE_CURVE)));
            window.animator().setFrame_display(target, true);
        },
    ));
}

fn ns_window(window: &WebviewWindow) -> Option<Retained<NSWindow>> {
    MainThreadMarker::new()?;
    let pointer = window.ns_window().ok()?.cast::<NSWindow>();
    // SAFETY: Tauri hands out the live NSWindow of this window, and the
    // marker above confirms the main thread, where AppKit may be used.
    unsafe { Retained::retain(pointer) }
}

/// The top edge of `frame`; AppKit's y axis points up.
fn top(frame: NSRect) -> f64 {
    frame.origin.y + frame.size.height
}

/// `frame` moved down by `points`.
fn shifted(frame: NSRect, points: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(frame.origin.x, frame.origin.y - points),
        frame.size,
    )
}

fn curve([x1, y1, x2, y2]: [f32; 4]) -> Retained<CAMediaTimingFunction> {
    CAMediaTimingFunction::functionWithControlPoints(x1, y1, x2, y2)
}
