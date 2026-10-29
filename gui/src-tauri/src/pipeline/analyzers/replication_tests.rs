use super::*;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn check<'a>(output: &'a Value, id: &str) -> &'a Value {
    output["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == id)
        .unwrap_or_else(|| panic!("missing check {id}"))
}

#[test]
fn a_typical_unready_package_gets_specific_findings() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "README.md",
        "# Replication\nData sources: the CPS.\nRun the code.\n",
    );
    write(
        root,
        "code/analysis.do",
        "* setup\ncd \"C:\\Users\\me\\project\"\nuse \"data/raw/cps.dta\", clear\nssc install reghdfe\ngen u = runiform()\nsave \"data\\clean\\cps.dta\", replace // backslashes\nesttab using \"output/table1.tex\", replace\n",
    );
    write(
        root,
        "code/helpers.R",
        "x <- read.csv(\"inputs/present.csv\")\ny <- sample(1:10)\n",
    );
    write(root, "code/inputs/present.csv", "a\n1\n");
    let output = run(root, &Value::Null).unwrap();

    assert_eq!(check(&output, "readme_present")["status"], "pass");
    assert_eq!(check(&output, "readme_sections")["status"], "fail");
    assert_eq!(check(&output, "main_script")["status"], "fail");
    let absolute = check(&output, "absolute_paths");
    assert_eq!(absolute["status"], "fail");
    assert_eq!(absolute["locations"][0]["path"], "code/analysis.do");
    assert_eq!(absolute["locations"][0]["line"], 2);
    assert_eq!(check(&output, "backslash_paths")["status"], "warn");
    assert_eq!(check(&output, "stata_packages")["status"], "pass");
    assert_eq!(check(&output, "r_packages")["status"], "warn");
    assert_eq!(check(&output, "random_seeds")["status"], "warn");
    let referenced = check(&output, "referenced_files");
    assert_eq!(referenced["status"], "warn");
    let snippets: Vec<&str> = referenced["locations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|location| location["snippet"].as_str().unwrap())
        .collect();
    assert!(snippets.iter().any(|line| line.contains("cps.dta")));
    assert!(!snippets.iter().any(|line| line.contains("present.csv")));
    assert_eq!(check(&output, "exported_outputs")["status"], "pass");
    assert_eq!(check(&output, "license")["status"], "warn");

    let findings = output["findings"].as_array().unwrap();
    let absolute_finding = findings
        .iter()
        .find(|finding| finding["id"] == "replication-absolute-paths")
        .unwrap();
    assert_eq!(absolute_finding["severity"], "high");
    assert_eq!(
        absolute_finding["evidence"][0]["source_path"],
        "code/analysis.do"
    );
    assert_eq!(absolute_finding["evidence"][0]["line_start"], 2);
    assert!(crate::findings::parse_value(&output, "package", "Package").is_some());
}

#[test]
fn a_prepared_package_passes_the_static_checks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "README.md",
        "## Data Availability and Provenance Statements\nRights and license: CC BY.\n## Computational requirements\nStata 18. Runtime: 2 hours; memory 16 GB.\nSeed set in main.do.\n## Description of programs\n## Instructions to Replicators\n## List of tables and programs\nTable 1: table1.tex\n## References\n",
    );
    write(root, "LICENSE", "MIT");
    write(
        root,
        "main.do",
        "global root = c(pwd)\nset seed 12345\nsysdir set PLUS \"$root/ado\"\ndo \"code/analysis.do\"\n",
    );
    write(root, "ado/plus/r/reghdfe.ado", "program reghdfe\nend\n");
    write(
        root,
        "code/analysis.do",
        "use \"$root/data/cps.dta\", clear\ngen u = runiform()\ngraph export \"$root/output/fig1.pdf\", replace\n",
    );
    let output = run(root, &Value::Null).unwrap();
    for id in [
        "readme_present",
        "readme_sections",
        "main_script",
        "absolute_paths",
        "backslash_paths",
        "stata_packages",
        "random_seeds",
        "referenced_files",
        "exported_outputs",
        "license",
    ] {
        assert_eq!(
            check(&output, id)["status"],
            "pass",
            "{id}: {}",
            check(&output, id)
        );
    }
    assert!(output["findings"].as_array().unwrap().is_empty());
}

#[test]
fn comments_symlinks_and_skipped_folders_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "main.do",
        "* cd \"C:\\old\\path\"\n// use \"gone.dta\"\ndisplay 1 // see C:/notes\n",
    );
    write(root, ".git/config", "use \"C:/Users/x/a.dta\"");
    write(root, "renv/library/pkg/R/x.R", "setwd(\"/Users/me\")");
    let output = run(root, &Value::Null).unwrap();
    assert_eq!(check(&output, "absolute_paths")["status"], "pass");
    assert_eq!(check(&output, "referenced_files")["status"], "pass");
    assert_eq!(output["files"], 1);
}

#[test]
fn a_file_input_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("paper.pdf");
    std::fs::write(&file, "x").unwrap();
    assert!(run(&file, &Value::Null).is_err());
}
