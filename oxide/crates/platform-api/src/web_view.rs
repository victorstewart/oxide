//! Headless web view service.

use crate::PlatformError;
use core::future::Future;

#[derive(Debug, Clone)]
pub enum WebViewEvent {
    /// The page has finished loading.
    LoadFinished,
    /// The page failed to load.
    LoadFailed(PlatformError),
}

pub trait WebView: Send + Sync {
    /// Executes a snippet of JavaScript in the context of the current page.
    /// The future resolves with the string result of the script's execution.
    fn execute_script(
        &self,
        script: &str,
    ) -> impl Future<Output = Result<Option<String>, PlatformError>> + Send;

    /// Closes and destroys the web view.
    fn close(&self);
}

pub trait WebViewService: Send + Sync {
    /// Creates a new, hidden web view and begins loading the specified URL.
    ///
    /// Events related to the web view's lifecycle (e.g., load finished/failed)
    /// are delivered via the provided callback.
    fn create_view(
        &self,
        url: &str,
        on_event: alloc::boxed::Box<dyn Fn(WebViewEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn WebView + Send>, PlatformError>;
}
