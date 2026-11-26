//! Slide-source rendering supplements native extraction; it never changes the
//! configured PDF extractor. Converters work in disposable copies, own bounded
//! child processes, and publish only a freshly validated PDF. The caller keeps
//! the workspace alive until it has retained the PDF and rendered page images.
use super::*;
use std::time::{Duration, Instant};

mod latex_source;
#[cfg(test)]
mod tests;
mod tools;
#[cfg(any(windows, test))]
mod windows;

const RENDER_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RENDERED_BYTES: u64 = 64 * 1024 * 1024;
const LOG_LIMIT: usize = 256 * 1024;
pub(crate) const SLIDE_PDF_PATH: &str = "artifacts/document/slides.pdf";
pub(crate) const SLIDE_LOG_PATH: &str = "context/slide-render.log";

pub(crate) struct SlideRender {
    pub pdf: Option<PathBuf>,
    pub log: PathBuf,
    pub note: String,
}

pub(crate) fn is_slide_source(extraction: &ExtractionResult) -> bool {
    extraction.method == "pptx"
        || (extraction.method == "latex"
            && Regex::new(r"\\documentclass\s*(?:\[[^\]]*\]\s*)?\{beamer\}")
                .unwrap()
                .is_match(&latex_without_comments(&extraction.text)))
}

pub(crate) fn render_slide_pdf(
    extraction: &ExtractionResult,
    workspace: &Path,
) -> Result<SlideRender, String> {
    let source = Path::new(&extraction.source_path);
    // Windows is explicitly native-only: absence is a launch error, not a
    // source-only review or a request to install/use WSL.
    #[cfg(windows)]
    if extraction.method == "latex" {
        let source_text = read_latex_capped(source, MAX_LATEX_SIZE)?.0;
        let engine = tools::tex_engine(&source_text)?;
        windows::required_engine(&engine)?;
    }
    let mut transcript = String::new();
    fs::create_dir_all(workspace).map_err(|e| e.to_string())?;
    let log = workspace.join("slide-render.log");
    let result = render(extraction, source, workspace, &mut transcript);
    let note = match &result {
        Ok(_) if extraction.method == "pptx" =>
            "PowerPoint was rendered with LibreOffice. The generated PDF and page images show the converted layout, including hidden slides; speaker notes remain separate. Font substitution or unsupported PowerPoint features may change appearance.".to_string(),
        Ok(_) => "Beamer was compiled to PDF. Use the generated PDF and page images for visual findings; cite PDF pages separately from source frames because overlays can change pagination.".to_string(),
        Err(error) => {
            if crate::cancellation::is_cancelled() || crate::pipeline::review_budget::stopped(error) {
                return Err(error.clone());
            }
            format!("Automatic slide rendering failed: {error}. This review has source content only; visual layout could not be verified. See the slide-render log.")
        }
    };
    transcript.push_str(&format!("\n{note}\n"));
    fs::write(&log, transcript).map_err(|e| format!("Cannot save slide rendering log: {e}"))?;
    Ok(SlideRender {
        pdf: result.ok(),
        log,
        note,
    })
}

fn render(
    extraction: &ExtractionResult,
    source: &Path,
    workspace: &Path,
    transcript: &mut String,
) -> Result<PathBuf, String> {
    crate::pipeline::review_budget::check()?;
    if crate::cancellation::is_cancelled() {
        return Err("Pipeline cancelled".into());
    }
    let output = workspace.join("output");
    // Never mistake a source companion or an earlier partial build for output.
    fs::create_dir(&output).map_err(|e| format!("Cannot create fresh slide output: {e}"))?;
    let started = Instant::now();
    let pdf = if extraction.method == "pptx" {
        let program = tools::libreoffice()
            .ok_or("Install LibreOffice to render PowerPoint decks automatically")?;
        let input = workspace.join("deck.pptx");
        let mut total = 0;
        latex::copy_scoped_source_file(source, &input, &mut total, MAX_STAGED_PDF_BYTES)?;
        let profile = workspace.join("office-profile");
        tools::office_profile(&profile)?;
        let mut command = program.command(tools::office_args(&input, &output, &profile)?);
        command.current_dir(workspace);
        run_converter(command, "LibreOffice", workspace, started, transcript)?;
        output.join("deck.pdf")
    } else {
        let root = source
            .parent()
            .ok_or("Beamer source has no parent directory")?;
        let build = workspace.join("source");
        latex_source::copy_project(root, &build)?;
        latex_source::rebase_references(root, &build)?;
        let filename = source.file_name().ok_or("Beamer source has no filename")?;
        let source_text = read_latex_capped(source, MAX_LATEX_SIZE)?.0;
        let engine = tools::tex_engine(&source_text)?;
        compile_native(&engine, filename, workspace, started, transcript)?;
        output.join(Path::new(filename).with_extension("pdf"))
    };
    validate_pdf(&pdf)?;
    Ok(pdf)
}

fn compile_native(
    engine: &str,
    filename: &std::ffi::OsStr,
    workspace: &Path,
    started: Instant,
    transcript: &mut String,
) -> Result<(), String> {
    let build = workspace.join("source");
    let output = workspace.join("output");
    fs::create_dir_all(workspace.join("tmp")).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let program = windows::required_engine(engine)?;
    #[cfg(not(windows))]
    let program = tools::tex_command(engine).ok_or_else(|| {
        format!("Install a TeX distribution with {engine} and Beamer to compile this deck")
    })?;
    #[cfg(windows)]
    let miktex = windows::is_miktex(&program, workspace, started)?;
    for pass in 1..=3 {
        let args = tools::tex_args(engine, filename, &output);
        #[cfg(windows)]
        let mut command = windows::command(&program, args, miktex);
        #[cfg(not(windows))]
        let mut command = tools::confined_tex_command(&program, &args, workspace)?;
        command.current_dir(&build);
        tools::tex_environment(&mut command, workspace, &output);
        transcript.push_str(&format!("\n{engine}, pass {pass}\n"));
        run_converter(command, engine, workspace, started, transcript)?;
    }
    Ok(())
}

fn run_converter(
    command: StdCommand,
    label: &str,
    workspace: &Path,
    started: Instant,
    transcript: &mut String,
) -> Result<(), String> {
    crate::pipeline::review_budget::check()?;
    if crate::cancellation::is_cancelled() {
        return Err("Pipeline cancelled".into());
    }
    let remaining = RENDER_TIMEOUT
        .checked_sub(started.elapsed())
        .ok_or("Slide rendering exceeded its two-minute time limit")?;
    let result = run_bounded_output(command, label, remaining, LOG_LIMIT, Some(workspace))?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    transcript.push_str(&stdout);
    transcript.push_str(&stderr);
    if result.stdout_truncated || result.stderr_truncated {
        transcript.push_str("\n[converter output truncated]\n");
    }
    crate::pipeline::review_budget::check()?;
    if crate::cancellation::is_cancelled() {
        return Err("Pipeline cancelled".into());
    }
    if !result.status.success() {
        return Err(format!("{label} exited with {}; check the slide-render log for missing packages, fonts, or source errors", result.status));
    }
    Ok(())
}

fn validate_pdf(path: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut file =
        open_regular_file(path).map_err(|e| format!("Converter produced no readable PDF: {e}"))?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > MAX_RENDERED_BYTES {
        return Err("Rendered slide PDF exceeds 64 MiB".into());
    }
    let mut header = [0; 5];
    file.read_exact(&mut header)
        .map_err(|e| format!("Rendered PDF is empty or truncated: {e}"))?;
    if &header != b"%PDF-" {
        return Err("Converter output is not a PDF".into());
    }
    // Parse the bounded bytes as well: a header alone cannot prove conversion
    // succeeded, nor can a zero-page PDF provide any visual evidence.
    let mut bytes = Vec::with_capacity(size as usize);
    bytes.extend_from_slice(&header);
    file.take(MAX_RENDERED_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RENDERED_BYTES {
        return Err("Rendered PDF grew past 64 MiB".into());
    }
    let document =
        lopdf::Document::load_mem(&bytes).map_err(|e| format!("Invalid rendered PDF: {e}"))?;
    if document.get_pages().is_empty() {
        return Err("Rendered slide PDF has no pages".into());
    }
    Ok(())
}
