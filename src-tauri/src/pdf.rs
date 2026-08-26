//! Render the app's own webview to a PDF file with WebView2's PrintToPdf.
//! Rust-side only: the webview gains no permission; the caller chooses the path.

use anyhow::{anyhow, Result};
use std::path::Path;

/// Renders the current page (print media, backgrounds on, scaled to fit) to `path`.
/// Must be called on the webview's thread — i.e. from inside `with_webview`.
#[cfg(windows)]
pub fn print_webview_to_pdf(wv: &tauri::webview::PlatformWebview, path: &Path) -> Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Environment6, ICoreWebView2_7,
    };
    use webview2_com::PrintToPdfCompletedHandler;
    use windows_core::{Interface, HSTRING};

    let core = unsafe { wv.controller().CoreWebView2()? };
    let core7: ICoreWebView2_7 = core.cast()?;
    let env6: ICoreWebView2Environment6 = wv.environment().cast()?;
    let settings = unsafe { env6.CreatePrintSettings()? };
    unsafe {
        settings.SetShouldPrintBackgrounds(true)?;
        settings.SetScaleFactor(0.8)?;
    }
    let target = HSTRING::from(path.to_string_lossy().as_ref());
    PrintToPdfCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            core7
                .PrintToPdf(&target, &settings, &handler)
                .map_err(Into::into)
        }),
        Box::new(|result: windows_core::Result<()>, ok: bool| {
            result?;
            if ok {
                Ok(())
            } else {
                // E_FAIL: WebView2 reported the print as unsuccessful.
                Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                    -2147467259,
                )))
            }
        }),
    )
    .map_err(|e| anyhow!("PrintToPdf failed: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn print_webview_to_pdf(_wv: &tauri::webview::PlatformWebview, _path: &Path) -> Result<()> {
    Err(anyhow!("PDF export is only available on Windows"))
}
