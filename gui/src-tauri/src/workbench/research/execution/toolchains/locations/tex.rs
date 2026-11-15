//! Native TeX distributions, including installations absent from a GUI's PATH.
use super::*;

pub(super) fn standard_locations(found: &mut Found<'_>) {
    let host = found.host;
    let mut bins = Vec::new();
    match host.platform {
        Platform::MacOs => {
            // frameworks holds /Library/Frameworks on a real Mac; derive the
            // sibling so tests can substitute an isolated Library folder.
            for frameworks in &host.frameworks {
                if let Some(library) = frameworks.parent() {
                    bins.push(library.join("TeX/texbin"));
                }
            }
        }
        Platform::Windows => {
            for programs in host.program_files.iter().chain(&host.user_programs) {
                for root in children(programs, |name| lower_starts(name, "miktex")) {
                    bins.push(root.join("miktex/bin/x64"));
                    bins.push(root.join("miktex/bin"));
                }
            }
        }
        Platform::Linux => {}
    }
    let mut roots: Vec<PathBuf> = host.prefixes.iter().map(|p| p.join("texlive")).collect();
    if host.platform.windows() {
        for programs in &host.program_files {
            if let Some(drive) = programs.parent() {
                roots.push(drive.join("texlive"));
            }
        }
    }
    for root in roots {
        for year in children(&root, |name| name.chars().all(|c| c.is_ascii_digit()))
            .into_iter()
            .rev()
        {
            bins.extend(children(&year.join("bin"), |_| true));
        }
    }
    for bin in bins {
        for engine in ["latexmk", "pdflatex", "xelatex", "lualatex"] {
            let name = if host.platform.windows() {
                format!("{engine}.exe")
            } else {
                engine.into()
            };
            found.push(
                "latex",
                bin.join(name),
                "standardLocation",
                engine.into(),
                None,
            );
        }
    }
}
