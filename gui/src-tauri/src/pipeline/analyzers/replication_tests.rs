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

#[test]
fn files_left_out_of_a_reviews_copy_are_still_part_of_the_package() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "code/analysis.do",
        "use \"data/panel.dta\", clear\ndo \"build/clean.do\"\n",
    );
    let mut omissions = crate::pipeline::extract::FolderOmissions::default();
    omissions.record_too_large(Path::new("code/data/panel.dta"), 3 * 1024 * 1024 * 1024);
    omissions.record_folder(Path::new("code/build"));
    omissions.save(root).unwrap();

    let output = run(root, &Value::Null).unwrap();

    let large = check(&output, "large_files");
    assert_eq!(large["status"], "info");
    assert_eq!(large["locations"][0]["path"], "code/data/panel.dta");
    assert_eq!(check(&output, "referenced_files")["status"], "pass");
    assert!(!output.to_string().contains(".pipeline_folder_omissions"));
}

fn status(output: &Value, id: &str) -> String {
    check(output, id)["status"].as_str().unwrap().to_string()
}

fn package_with(path: &str, code: &str) -> Value {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), path, code);
    run(dir.path(), &Value::Null).unwrap()
}

#[test]
fn files_a_package_writes_or_finds_by_a_relative_path_are_not_missing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "data/x.csv", "a\n1\n");
    write(root, "data/raw.dta", "x");
    write(
        root,
        "code/analysis.do",
        r#"log using "main.log", replace
capture log using "logs/second.log"
use "../data/raw.dta", clear
file open fh using "out.tex", write replace
tabout region using "t.txt", replace
save "data/clean.dta", replace
use "data/clean.dta", clear
save "derived/merged"
merge 1:1 id using "derived/merged.dta"
"#,
    );
    write(root, "code/load.R", "x <- read.csv(\"../data/x.csv\")\n");
    write(
        root,
        "code/write.py",
        "with open(\"out.csv\", \"w\") as f:\n    f.write(\"a\")\nopen('log.txt', mode='a')\nopen(\"out.csv\", \"r\")\n",
    );
    let output = run(root, &Value::Null).unwrap();
    assert_eq!(
        status(&output, "referenced_files"),
        "pass",
        "{}",
        check(&output, "referenced_files")
    );
}

#[test]
fn files_read_but_never_supplied_or_written_are_still_missing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "data/x.csv", "a\n1\n");
    write(
        root,
        "code/analysis.do",
        "merge 1:1 id using \"data/absent.dta\"\nuse \"../other/raw.dta\"\nfile open fh using \"notes.txt\", read\nsaved_list = 1\n",
    );
    write(
        root,
        "code/read.py",
        "saved = pd.read_csv(\"gone.csv\")\nf = open(\"also_gone.csv\", \"r\")\n",
    );
    let output = run(root, &Value::Null).unwrap();
    let referenced = check(&output, "referenced_files");
    assert_eq!(referenced["status"], "warn");
    assert_eq!(referenced["locations"].as_array().unwrap().len(), 5);
}

#[test]
fn a_truncated_file_list_reports_no_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "a_analysis.do", "use \"z_data.dta\"\n");
    write(root, "b.txt", "x");
    write(root, "z_data.dta", "x");
    let output = run(root, &json!({"max_files": 2})).unwrap();
    assert_eq!(output["inventory_truncated"], true);
    let referenced = check(&output, "referenced_files");
    assert_eq!(referenced["status"], "info");
    assert!(referenced["locations"].is_null());
    assert!(!output["findings"]
        .to_string()
        .contains("not in the package"));
}

#[test]
fn stata_seed_warnings_follow_the_sample_command_and_not_the_word() {
    for deterministic in [
        "reg y x\nkeep if e(sample)\n",
        "gen byte sample = 1\nreg y x if sample == 1\nsum y if sample\n",
        "display \"bootstrap and simulate\"\n",
        "bysort id: gen sample2 = _n\n",
        "gen runiform_flag = 1\n",
    ] {
        let output = package_with("main.do", deterministic);
        assert_eq!(status(&output, "random_seeds"), "pass", "{deterministic}");
    }
    for random in [
        "sample 10\n",
        "  quietly sample 25, count\n",
        "gen u = runiform()\n",
        "gen z = rnormal(0, 1)\n",
        "bootstrap, reps(100): reg y x\n",
        "bysort id: sample 1, count\n",
        "reg y x, vce(bootstrap, reps(50))\n",
    ] {
        let output = package_with("main.do", random);
        assert_eq!(status(&output, "random_seeds"), "warn", "{random}");
        let seeded = package_with("main.do", &format!("set seed 42\n{random}"));
        assert_eq!(status(&seeded, "random_seeds"), "pass", "{random}");
    }
    let output = package_with("main.do", "bootstrap, reps(100) seed(7): reg y x\n");
    assert_eq!(status(&output, "random_seeds"), "pass");
}

#[test]
fn numpy_and_julia_seeds_are_recognized_however_they_are_passed() {
    for seeded in [
        "import numpy as np\nSEED = 7\nrng = np.random.default_rng(SEED)\nx = rng.normal(size=3)\n",
        "rng = np.random.default_rng(seed=7)\n",
        "rng = np.random.default_rng(12345)\n",
        "rng = np.random.default_rng(\n    SEED,\n)\n",
        "np.random.seed(1)\nx = np.random.normal()\n",
    ] {
        let output = package_with("main.py", seeded);
        assert_eq!(status(&output, "random_seeds"), "pass", "{seeded}");
    }
    for unseeded in [
        "rng = np.random.default_rng()\n",
        "x = np.random.normal()\nreseed(1)\n",
    ] {
        let output = package_with("main.py", unseeded);
        assert_eq!(status(&output, "random_seeds"), "warn", "{unseeded}");
    }
    let output = package_with("main.jl", "using Random\nseed!(1)\nx = rand(3)\n");
    assert_eq!(status(&output, "random_seeds"), "pass");
    let output = package_with("main.jl", "using Random\nRandom.seed!(1)\nx = rand(3)\n");
    assert_eq!(status(&output, "random_seeds"), "pass");
    let output = package_with("main.jl", "x = rand(3)\n");
    assert_eq!(status(&output, "random_seeds"), "warn");
}

#[test]
fn readme_keywords_match_whole_words() {
    assert!(!mentions("the programs run in order", "ram"));
    assert!(mentions("needs 16 gb of ram.", "ram"));
    assert!(mentions("(ram)", "ram"));
    assert!(mentions("random seeds are set", "seed"));
    assert!(!mentions("programme", "program"));
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "README.md",
        "These programs replicate the paper.\n",
    );
    let output = run(dir.path(), &Value::Null).unwrap();
    let detail = check(&output, "readme_sections")["detail"]
        .as_str()
        .unwrap();
    assert!(detail.contains("Memory, runtime, and storage"), "{detail}");
    assert!(!detail.contains("Description of programs"), "{detail}");
}

#[test]
fn stata_latex_strings_are_not_backslash_paths_but_windows_paths_are() {
    for latex in [
        r#"esttab using "t.tex", postfoot("\hline\hline \end{tabular}") replace"#,
        r#"estout using "t.tex", prehead("\begin{table}[h]\centering") replace"#,
        r#"esttab m1 using "t.tex", title("Effects\label{tab:a}") substitute(\_ _)"#,
        r#"local fmt "\hline""#,
        r#"file write fh "\midrule" _n"#,
        r#"file write fh "a & b \\" _n"#,
        r#"display "tab\tseparated""#,
    ] {
        let output = package_with("main.do", &format!("{latex}\n"));
        assert_eq!(status(&output, "backslash_paths"), "pass", "{latex}");
    }
    for path in [
        r#"cd "C:\project\data""#,
        r#"use "data\raw\cps.dta", clear"#,
        r#"save "data\clean", replace"#,
        r#"global root "D:\work\paper""#,
        r#"graph export "output\fig1.pdf", replace"#,
        r#"esttab using "output\t1.tex", replace"#,
        r#"do "code\clean""#,
    ] {
        let output = package_with("main.do", &format!("{path}\n"));
        assert_eq!(status(&output, "backslash_paths"), "warn", "{path}");
    }
}

#[test]
fn stata_block_comments_hide_their_contents() {
    for commented in [
        "/* global root \"C:/Users/me/project\" */\ndisplay 1\n",
        "/*\nglobal root \"C:/Users/me/project\"\ncd \"/home/me\"\n*/\ndisplay 1\n",
        "/* outer /* inner */\ncd \"C:/Users/me\"\n*/\ndisplay 1\n",
        "display 1 /* cd \"C:/Users/me\" */\n",
    ] {
        let output = package_with("main.do", commented);
        assert_eq!(status(&output, "absolute_paths"), "pass", "{commented}");
    }
    for live in [
        "/* header */\ncd \"C:/Users/me\"\n",
        "/* one\ntwo */ cd \"C:/Users/me\"\n",
        "use \"data/*.dta\"\ncd \"C:/Users/me\"\n",
        "* a star comment /* never closed\ncd \"C:/Users/me\"\n",
        "display 1 // not a block /* comment\ncd \"C:/Users/me\"\n",
    ] {
        let output = package_with("main.do", live);
        assert_eq!(status(&output, "absolute_paths"), "fail", "{live}");
    }
    let output = package_with(
        "main.do",
        "/*\nuse \"gone.dta\"\ngen u = runiform()\n*/\ndisplay 1\n",
    );
    assert_eq!(status(&output, "referenced_files"), "pass");
    assert_eq!(status(&output, "random_seeds"), "pass");
}

#[test]
fn block_comment_syntax_means_nothing_outside_stata() {
    let output = package_with("main.py", "x = 2 /* 3\nopen(\"C:/Users/me/a.txt\")\n");
    assert_eq!(status(&output, "absolute_paths"), "fail");
}
