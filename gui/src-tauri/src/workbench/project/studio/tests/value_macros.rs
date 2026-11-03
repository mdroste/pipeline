//! Staged value macros build up one file. A generated file merges by macro name,
//! and a file with anything else in it is left alone.
use super::*;

fn project_action(f: &Fixture, action: ProjectAction) -> Value {
    mutate(
        &f.store,
        ProjectMutation {
            workspace_id: f.ws.clone(),
            operation_id: id("action").unwrap(),
            action,
        },
    )
    .unwrap()
}
struct Copy {
    id: String,
    root: PathBuf,
}
fn task_copy(f: &Fixture, paths: Vec<String>) -> Copy {
    let task = project_action(
        f,
        ProjectAction::CreateTask {
            objective: "Update the values".into(),
            anchor_id: None,
            expected_outputs: vec![],
            expected_checks: vec![],
        },
    );
    let copy = project_action(
        f,
        ProjectAction::Checkpoint {
            task_id: task["id"].as_str().unwrap().into(),
            paths,
            backend: "copy".into(),
        },
    );
    Copy {
        id: copy["id"].as_str().unwrap().into(),
        root: copy["body"]["taskRoot"].as_str().unwrap().into(),
    }
}
fn stage(f: &Fixture, copy: &Copy, name: &str, result: &ResultRef) -> WorkbenchResult<Value> {
    generate_values(
        &f.store,
        &f.ws,
        &copy.id,
        "values.tex",
        vec![ValueMacro {
            name: name.into(),
            result: result.clone(),
            precision: 2,
        }],
    )
}

#[test]
fn a_second_macro_joins_the_first_and_a_repeat_replaces_only_its_own() {
    let f = fixture();
    let (_, result) = exported_result(&f, 0.1234, "log points");
    fs::write(f.root.join("main.tex"), "text").unwrap();
    let copy = task_copy(&f, vec!["main.tex".into()]);
    let file = copy.root.join("values.tex");

    stage(&f, &copy, "HeadlineEstimate", &result).unwrap();
    stage(&f, &copy, "SecondEstimate", &result).unwrap();
    let both = fs::read_to_string(&file).unwrap();
    assert!(both.contains("\\newcommand{\\HeadlineEstimate}{0.12}"));
    assert!(both.contains("\\newcommand{\\SecondEstimate}{0.12}"));
    assert_eq!(both.matches("% Generated from").count(), 1);

    generate_values(
        &f.store,
        &f.ws,
        &copy.id,
        "values.tex",
        vec![ValueMacro {
            name: "HeadlineEstimate".into(),
            result,
            precision: 3,
        }],
    )
    .unwrap();
    let merged = fs::read_to_string(&file).unwrap();
    assert!(merged.contains("\\newcommand{\\HeadlineEstimate}{0.123}"));
    assert!(merged.contains("\\newcommand{\\SecondEstimate}{0.12}"));
    assert_eq!(merged.matches("\\newcommand").count(), 2);
    assert!(
        merged.find("HeadlineEstimate") < merged.find("SecondEstimate"),
        "{merged}"
    );
}

#[test]
fn a_file_with_other_content_is_refused_and_left_as_it_was() {
    let f = fixture();
    let (_, result) = exported_result(&f, 0.1234, "log points");
    let original = "\\newcommand{\\Mine}{by hand}\n";
    fs::write(f.root.join("values.tex"), original).unwrap();
    let copy = task_copy(&f, vec!["values.tex".into()]);

    let error = stage(&f, &copy, "HeadlineEstimate", &result).unwrap_err();
    assert!(
        error.message.starts_with("values.tex holds text"),
        "{error:?}"
    );
    assert_eq!(
        fs::read_to_string(copy.root.join("values.tex")).unwrap(),
        original
    );
}

#[test]
fn a_researchers_edit_inside_a_generated_file_is_not_overwritten() {
    let f = fixture();
    let (_, result) = exported_result(&f, 0.1234, "log points");
    fs::write(f.root.join("main.tex"), "text").unwrap();
    let copy = task_copy(&f, vec!["main.tex".into()]);
    stage(&f, &copy, "HeadlineEstimate", &result).unwrap();
    let file = copy.root.join("values.tex");
    let edited = format!(
        "{}\\newcommand{{\\Extra}}{{by hand}}\n",
        fs::read_to_string(&file).unwrap()
    );
    fs::write(&file, &edited).unwrap();

    assert!(stage(&f, &copy, "SecondEstimate", &result).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), edited);
}

#[test]
fn an_empty_file_is_filled_in() {
    let f = fixture();
    let (_, result) = exported_result(&f, 0.1234, "log points");
    fs::write(f.root.join("values.tex"), "\n").unwrap();
    let copy = task_copy(&f, vec!["values.tex".into()]);
    stage(&f, &copy, "HeadlineEstimate", &result).unwrap();
    assert!(fs::read_to_string(copy.root.join("values.tex"))
        .unwrap()
        .contains("\\newcommand{\\HeadlineEstimate}{0.12}"));
}
