//! Source extraction adapters and PDF rendering.

use crate::env;
use crate::models::ExtractionResult;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::LazyLock;

mod core;
mod dispatch;
mod document;
mod folder;
mod latex;
mod pdf;
mod rendering;
mod staged;
mod structured;

use core::*;
use document::*;
use latex::*;
use pdf::*;
#[cfg(test)]
use rendering::rendered_page_limit;
use structured::*;

pub use core::MAX_RENDERED_PDF_PAGES;
pub(crate) use core::{find_main_tex, ScopedSourceContext};
pub use dispatch::extract;
pub use folder::{effective_input_mode, ingest_folder, ingest_folder_async, ingest_none};
pub(crate) use latex::stage_selected_source;
pub(crate) use pdf::extract_pdftotext;
pub use rendering::{
    render_pdf_page_preview, render_pdf_pages, RenderedPdfPagePreview, RenderedPdfPages,
};
pub(crate) use staged::reference as captured_reference;
#[allow(unused_imports)]
pub(crate) use structured::{
    paddle_full_image_inventory, read_paddle_structure_for_method,
    read_paddle_structure_json_for_method, PaddleStructure, PaddleStructuredBlock,
    PaddleStructuredPage,
};

#[cfg(test)]
mod tests;
