//! Validation and delivery of explicitly published workflow products.
use super::*;

/// Legacy issue arrays and open schemas remain valid. A schema describing a
/// specific object must declare the collection its findings reader consumes.
fn accepts_findings_schema(schema: &Value) -> bool {
    fn admits(schema: &Value, kind: &str) -> bool {
        match schema.get("type") {
            Some(Value::String(value)) => value == kind,
            Some(Value::Array(values)) => values.iter().any(|value| value == kind),
            _ => true,
        }
    }
    if admits(schema, "array") && schema.get("properties").is_none() && schema.get("type").is_some()
    {
        return schema
            .get("items")
            .is_none_or(|item| admits(item, "object"));
    }
    if !admits(schema, "object") {
        return false;
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    if let Some(fields) = properties.filter(|fields| !fields.is_empty()) {
        return ["findings", "issues"].iter().any(|key| {
            fields.get(*key).is_some_and(|field| {
                admits(field, "array")
                    && field.get("items").is_none_or(|item| admits(item, "object"))
            })
        });
    }
    schema.get("additionalProperties") != Some(&Value::Bool(false))
}

/// New saves and imports can diagnose a type mismatch immediately. Retained
/// workflow snapshots use the historical reader and diagnose delivery at run time.
pub(crate) fn validate_declaration(
    steps: &[crate::pipeline_config::StepConfig],
    producer: &str,
) -> Result<(), String> {
    if let Some(schema) = steps
        .iter()
        .find(|s| s.id == producer)
        .and_then(|s| s.output_schema.as_ref())
    {
        let schema = crate::pipeline::structured::resolve_schema_reference(schema)?;
        if !accepts_findings_schema(&schema) {
            return Err(format!("Published findings step '{producer}' must produce an array or an object containing a findings or issues array"));
        }
    }
    Ok(())
}

/// Attribute a missing promised result to its producer so history, the CLI,
/// Tasks and Resume all see the same failure. Undeclared inferred products
/// retain their legacy best-effort behavior.
pub(crate) fn check_publication(
    config: &PipelineConfig,
    outputs: &[StepOutput],
    products: &RunProducts,
    failures: &mut Vec<crate::models::StepFailure>,
) {
    let mut missing = |producer: &str, product: &str| {
        if producer.is_empty()
            || failures
                .iter()
                .any(|failure| base_step_id(&failure.step_id) == producer)
        {
            return;
        }
        if let Some(step) = config.steps.iter().find(|step| step.id == producer) {
            let reason = if outputs
                .iter()
                .any(|o| base_step_id(&o.step_id) == producer && o.skipped)
            {
                "The producer was skipped."
            } else if declared_output(outputs, producer).is_some() {
                "Its output did not satisfy the product contract."
            } else {
                "The producer has no usable output."
            };
            failures.push(crate::models::StepFailure {
                step_id: step.id.clone(),
                step_label: step.label.clone(),
                phase: "publication".into(),
                error: format!(
                    "The workflow did not deliver its declared {product} from '{}'. {reason}",
                    step.label
                ),
            });
        }
    };
    if declared_output(outputs, &config.outputs.primary_step).is_none() {
        missing(&config.outputs.primary_step, "primary report");
    }
    if products.findings.is_none() {
        missing(&config.outputs.findings_step, "findings");
    }
    for spec in &config.outputs.named {
        // Conditional named products have historically been optional when
        // their producer is skipped. A producer that ran must publish valid data.
        if (spec.required || declared_output(outputs, &spec.step).is_some())
            && !products.named.iter().any(|product| product.key == spec.key)
        {
            missing(&spec.step, &format!("product '{}'", spec.key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conditional_named_products_remain_optional_until_required() {
        let mut config = crate::pipeline_config::workflow_template().unwrap().config;
        let producer = config.steps[0].id.clone();
        config.outputs.primary_step.clear();
        config.outputs.findings_step.clear();
        config.outputs.named = vec![crate::pipeline_config::NamedProductSpec {
            key: "score".into(),
            step: producer.clone(),
            ..Default::default()
        }];
        let skipped = [StepOutput {
            step_id: producer.clone(),
            skipped: true,
            ..Default::default()
        }];
        let products = build_run_products(&config, &skipped);
        let mut failures = Vec::new();
        check_publication(&config, &skipped, &products, &mut failures);
        assert!(failures.is_empty());
        assert!(serde_json::to_value(&config.outputs.named[0])
            .unwrap()
            .get("required")
            .is_none());
        config.outputs.named[0].required = true;
        check_publication(&config, &skipped, &products, &mut failures);
        assert_eq!(failures[0].step_id, producer);
        assert!(failures[0].error.contains("skipped"));
    }

    #[test]
    fn publication_distinguishes_absent_findings_from_an_empty_collection() {
        let mut config = crate::pipeline_config::workflow_template().unwrap().config;
        let id = config.outputs.primary_step.clone();
        config.outputs.findings_step = id.clone();
        for (raw, failed) in [
            (r#"{"score":1}"#, true),
            (r#"{"findings":[]}"#, false),
            (r#"{"issues":[]}"#, false),
        ] {
            let mut produced = StepOutput {
                structured_json: true,
                raw_text: raw.to_string(),
                ..Default::default()
            };
            produced.step_id = id.clone();
            let outputs = [produced];
            let products = build_run_products(&config, &outputs);
            let mut failures = Vec::new();
            check_publication(&config, &outputs, &products, &mut failures);
            assert_eq!(!failures.is_empty(), failed);
            if failed {
                assert_eq!(failures[0].step_id, id);
            }
        }
    }
}
