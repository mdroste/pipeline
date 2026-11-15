//! WebKitGTK prints through GTK's file backend, never the default printer.
use super::*;
use gtk::prelude::GtkSettingsExt;
use std::{cell::RefCell, rc::Rc};
use webkit2gtk::{PrintOperationExt, SettingsExt as WebKitSettingsExt, WebViewExt};

pub(super) fn print(view: tauri::webview::PlatformWebview, path: PathBuf, done: Completion) {
    let Ok(url) = tauri::Url::from_file_path(path) else {
        complete(&done, Err("Invalid PDF staging URL".into()));
        return;
    };
    let Some(gtk_settings) = gtk::Settings::default() else {
        complete(&done, Err("Linux print settings are unavailable".into()));
        return;
    };
    // Only load the file backend in this isolated process. A missing PDF
    // backend must fail instead of ever falling back to a physical printer.
    gtk_settings.set_gtk_print_backends(Some("file"));
    let settings = gtk::PrintSettings::new();
    settings.set_printer(&gtk::glib::dgettext(Some("gtk30"), "Print to File"));
    settings.set("output-file-format", Some("pdf"));
    settings.set("output-uri", Some(url.as_str()));
    let page = gtk::PageSetup::new();
    page.set_paper_size(&gtk::PaperSize::new(Some("na_letter")));
    page.set_orientation(gtk::PageOrientation::Portrait);
    page.set_top_margin(TOP, gtk::Unit::Points);
    page.set_bottom_margin(BOTTOM, gtk::Unit::Points);
    page.set_left_margin(SIDE, gtk::Unit::Points);
    page.set_right_margin(SIDE, gtk::Unit::Points);
    let webview = view.inner();
    if let Some(settings) = webview.settings() {
        settings.set_print_backgrounds(true);
    }
    let operation = webkit2gtk::PrintOperation::new(&webview);
    operation.set_print_settings(&settings);
    operation.set_page_setup(&page);
    let failure = done.clone();
    operation.connect_failed(move |_, error| {
        complete(&failure, Err(format!("Linux PDF export failed: {error}")))
    });
    // Retain the operation while printing, then break the signal/owner cycle.
    let owner = Rc::new(RefCell::new(Some(operation.clone())));
    operation.connect_finished(move |_| {
        complete(&done, Ok(()));
        owner.borrow_mut().take();
    });
    operation.print();
}
