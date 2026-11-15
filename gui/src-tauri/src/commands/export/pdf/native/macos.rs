//! WebKit's paginated print operation, with a private print ticket and no UI.
use super::*;
use objc2::{
    define_class, msg_send, rc::Retained, runtime::ProtocolObject, sel, AllocAnyThread,
    MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSPaperOrientation, NSPrintInfo, NSPrintJobSavingURL, NSPrintOperation, NSPrintSaveJob,
    NSPrintingPaginationMode,
};
use objc2_foundation::{NSObject, NSSize, NSString, NSURL};
use objc2_web_kit::WKWebView;
use std::ffi::c_void;

struct RunningPrint {
    _operation: Retained<NSPrintOperation>,
    _delegate: Retained<PdfPrintDelegate>,
    done: Completion,
}

define_class!(
    // NSObject has no additional subclassing requirements. AppKit invokes
    // this private completion selector once, on the main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct PdfPrintDelegate;

    impl PdfPrintDelegate {
        #[unsafe(method(printOperationDidRun:success:contextInfo:))]
        fn finished(&self, _operation: &NSPrintOperation, success: bool, context: *mut c_void) {
            // The context owns both objects until AppKit completes the job.
            let running = unsafe { Box::from_raw(context.cast::<RunningPrint>()) };
            complete(&running.done, if success { Ok(()) } else {
                Err("macOS could not finish printing the report to PDF".into())
            });
        }
    }
);

pub(super) fn print(view: tauri::webview::PlatformWebview, path: PathBuf, done: Completion) {
    // Tauri calls this closure on the UI thread and keeps the native view alive.
    // The operation and its print ticket are confined to this helper process.
    unsafe {
        let webview = &*view.inner().cast::<WKWebView>();
        let info = NSPrintInfo::init(NSPrintInfo::alloc());
        info.setPaperSize(NSSize::new(WIDTH, HEIGHT));
        info.setOrientation(NSPaperOrientation::Portrait);
        info.setScalingFactor(1.);
        info.setTopMargin(TOP);
        info.setBottomMargin(BOTTOM);
        info.setLeftMargin(SIDE);
        info.setRightMargin(SIDE);
        info.setHorizontallyCentered(false);
        info.setVerticallyCentered(false);
        info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
        info.setVerticalPagination(NSPrintingPaginationMode::Automatic);
        info.setJobDisposition(NSPrintSaveJob);
        let destination = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        info.dictionary()
            .setObject_forKey(&destination, ProtocolObject::from_ref(NSPrintJobSavingURL));
        let operation = webview.printOperationWithPrintInfo(&info);
        operation.setShowsPrintPanel(false);
        operation.setShowsProgressPanel(false);
        let Some(window) = webview.window() else {
            complete(&done, Err("The PDF rendering window closed".into()));
            return;
        };
        let delegate: Retained<PdfPrintDelegate> = msg_send![
            PdfPrintDelegate::alloc(MainThreadMarker::new().expect("Tauri UI thread")),
            init
        ];
        let running = Box::new(RunningPrint {
            _operation: operation.clone(),
            _delegate: delegate.clone(),
            done,
        });
        // WebKit needs its asynchronous printing thread to compute page ranges
        // while the UI loop services IPC. Synchronous runOperation can print
        // unbounded placeholder pages instead. Panels remain disabled.
        operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
            &window,
            Some(&delegate),
            Some(sel!(printOperationDidRun:success:contextInfo:)),
            Box::into_raw(running).cast(),
        );
    }
}
