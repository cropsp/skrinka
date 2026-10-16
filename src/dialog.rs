//! Message boxes. They are modal and block their thread, so the event-loop thread only ever
//! uses the `_async` variants.

use rfd::{MessageButtons, MessageDialog, MessageLevel};

/// Blocking; for worker threads.
pub fn error(text: &str) {
    show(MessageLevel::Error, text);
}

/// Shows the error from a short-lived thread; safe to call from winit callbacks.
pub fn error_async(text: String) {
    std::thread::spawn(move || error(&text));
}

fn show(level: MessageLevel, text: &str) {
    let _ = MessageDialog::new()
        .set_level(level)
        .set_title("Skrinka")
        .set_description(text)
        .set_buttons(MessageButtons::Ok)
        .show();
}
