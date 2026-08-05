use objc2::AllocAnyThread;
use objc2_app_kit::{NSTrackingArea, NSTrackingAreaOptions, NSView};
use tauri::{Runtime, Webview};

fn inactive_hover_tracking_options() -> NSTrackingAreaOptions {
    NSTrackingAreaOptions::MouseEnteredAndExited
        | NSTrackingAreaOptions::MouseMoved
        | NSTrackingAreaOptions::ActiveAlways
        | NSTrackingAreaOptions::InVisibleRect
}

pub fn enable_inactive_hover<R: Runtime>(window: &Webview<R>) -> tauri::Result<()> {
    window.with_webview(|webview| unsafe {
        let view: &NSView = &*webview.inner().cast();
        add_inactive_hover_tracking(view);
    })
}

unsafe fn add_inactive_hover_tracking(view: &NSView) {
    let options = inactive_hover_tracking_options();
    let already_installed = view
        .trackingAreas()
        .into_iter()
        .any(|area| area.options() == options);

    if !already_installed {
        let area = NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            view.bounds(),
            options,
            Some(view),
            None,
        );
        view.addTrackingArea(&area);
    }

    for child in view.subviews() {
        add_inactive_hover_tracking(&child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_hover_tracking_stays_active_outside_the_frontmost_app() {
        let options = inactive_hover_tracking_options();

        assert!(options.contains(NSTrackingAreaOptions::MouseEnteredAndExited));
        assert!(options.contains(NSTrackingAreaOptions::MouseMoved));
        assert!(options.contains(NSTrackingAreaOptions::ActiveAlways));
        assert!(options.contains(NSTrackingAreaOptions::InVisibleRect));
    }
}
