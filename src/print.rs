use anyhow::{Context, Result};

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub fn show(html: String) -> Result<()> {
    use tao::{
        dpi::LogicalSize,
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoopBuilder},
        platform::run_return::EventLoopExtRunReturn,
        window::WindowBuilder,
    };
    use wry::{PageLoadEvent, WebViewBuilder};

    #[cfg(target_os = "windows")]
    anyhow::ensure!(
        html.len() < 2 * 1024 * 1024,
        "receipt exceeds WebView2's 2 MiB HTML limit"
    );
    #[cfg(target_os = "linux")]
    gtk::init().context("printing requires a graphical desktop with GTK and WebKitGTK")?;

    let mut event_loop = EventLoopBuilder::<()>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("Codex receipt — close this window when finished")
        .with_inner_size(LogicalSize::new(460, 760))
        .build(&event_loop)
        .context("opening receipt window")?;
    let builder = WebViewBuilder::new()
        .with_html(html)
        .with_incognito(true)
        .with_javascript_disabled()
        .with_on_page_load_handler(move |event, _| {
            if matches!(event, PageLoadEvent::Finished) {
                let _ = proxy.send_event(());
            }
        });
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let webview = builder.build(&window);
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(
            window
                .default_vbox()
                .context("receipt window has no GTK container")?,
        )
    };
    let webview = webview.context(
        "loading receipt (Windows requires the WebView2 Runtime; Linux requires WebKitGTK 4.1)",
    )?;
    let mut opened = false;
    let mut result = Ok(());
    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(()) if !opened => {
                opened = true;
                if let Err(error) = open_dialog(&webview) {
                    result = Err(error.context("opening native print dialog"));
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
    result
}

#[cfg(target_os = "windows")]
fn open_dialog(webview: &wry::WebView) -> Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PRINT_DIALOG_KIND_SYSTEM, ICoreWebView2_16,
    };
    use windows_core::Interface;
    use wry::WebViewExtWindows;

    let native = webview.webview().cast::<ICoreWebView2_16>()?;
    // The WebView2 interface belongs to this thread's running UI event loop.
    unsafe { native.ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_SYSTEM)? };
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn open_dialog(webview: &wry::WebView) -> Result<()> {
    webview.print().context("printing receipt")
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn show(_html: String) -> Result<()> {
    anyhow::bail!("native print dialogs are supported on macOS, Windows, and Linux")
}
