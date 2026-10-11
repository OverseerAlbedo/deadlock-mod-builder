use deadlock_model::{
    self as model, modeldoc, source_skeleton::SourceAuditReport, Mappings, SourcePaths,
};
use deadlock_project::LodStrategy;
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/reference.json")).unwrap()
}
fn typed<T: serde::de::DeserializeOwned>(value: &Value) -> T {
    serde_json::from_value(value.clone()).unwrap()
}
fn perform(case: &Value) -> model::Result<Value> {
    let source = case["source"].as_str().unwrap_or_default();
    Ok(match case["operation"].as_str().unwrap() {
        "equipment" => json!(model::equipment::apply_mappings(
            &model::analyze(source)?,
            &typed::<Mappings>(&case["mappings"]),
            &typed::<deadlock_project::Customization>(&case["state"]),
            &typed::<std::collections::BTreeSet<String>>(&case["source_names"])
        )?),
        "equipment_project" => json!(model::equipment::project_mappings(
            &model::analyze(source)?,
            &typed::<Mappings>(&case["effective"]),
            &typed::<Mappings>(&case["baseline"]),
            &typed::<deadlock_project::Customization>(&case["state"])
        )?),
        "analyze" => serde_json::to_value(model::analyze(source)?).unwrap(),
        "patch" => json!(model::patch(
            source,
            &typed::<Mappings>(&case["mappings"]),
            &typed::<BTreeMap<String, String>>(&case["source_paths"])
        )?),
        "merge" => {
            let report = case.get("original_report").map(typed::<SourceAuditReport>);
            let targets = case
                .get("template_targets")
                .map(typed::<BTreeMap<String, String>>);
            json!(model::merge_template(
                source,
                case["template"].as_str().unwrap(),
                &typed::<Mappings>(&case["mappings"]),
                &typed::<SourcePaths>(&case["source_path"]),
                typed::<LodStrategy>(&case["strategy"]),
                targets.as_ref(),
                report.as_ref()
            )?)
        }
        "suggest" => {
            let (mappings, unresolved) = model::suggest_mappings(
                &model::analyze(source)?,
                &typed::<Vec<String>>(&case["replacements"]),
                &case
                    .get("mappings")
                    .map(typed::<Mappings>)
                    .unwrap_or_default(),
                case.get("strategy")
                    .map(typed::<LodStrategy>)
                    .unwrap_or_default(),
                case["auto"].as_bool().unwrap_or(true),
            )?;
            json!({"mappings":mappings,"unresolved":unresolved})
        }
        "plan" => serde_json::to_value(modeldoc::migration_plan(
            source,
            case["target"].as_u64().map(|v| v as u32),
        )?)
        .unwrap(),
        "references" => json!(model::source_references(source)?),
        "isolated" => json!(model::isolated_filename(
            case["hero_root"].as_str().unwrap(),
            case["source_name"].as_str().unwrap()
        )?),
        "diagnostic" => json!(modeldoc::diagnostic(case["output"].as_str().unwrap())),
        _ => panic!("unknown operation"),
    })
}
#[test]
fn python_golden_all_authored_bytes_and_mapping_semantics() {
    for case in fixture()["cases"].as_array().unwrap() {
        let result = perform(case);
        if case.get("error").is_some() {
            assert!(
                result.is_err(),
                "accepted reference rejection: {}",
                case["name"]
            );
        } else {
            let output = result.unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
            assert_eq!(output, case["expected"], "reference case {}", case["name"]);
        }
    }
}
#[test]
fn fresh_probe_diagnostic_is_required_even_for_success_exit() {
    for target in [40, 41] {
        let output=format!("RESOURCE COMPILE ERROR:\nNo valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc{target}'");
        assert_eq!(modeldoc::interpret_probe(&output).unwrap(), target);
    }
    for output in [
        "",
        "1 compiled, 0 failed",
        "No valid format conversion from 'other_source' to 'modeldoc40'",
        "No valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc42'",
        "ParticleFloatType_t\nMismatch - Member Count",
    ] {
        assert!(modeldoc::interpret_probe(output).is_err(), "{output}");
    }
    let source = modeldoc::probe_source();
    assert!(source.contains("dmb_modeldoc_probe"));
    assert!(source.ends_with("\n"));
}
#[test]
fn split_does_not_edit_original_transform_or_unknown_metadata() {
    let source="{ nodes = [ { _class = \"RenderMeshFile\" name = \"body\" filename = \"old.dmx\" import_scale = 1.0000238418579102 custom = { preserve = #[00ff] } }, { _class = \"Attachment\" name = \"eyes\" origin = [1.5, 2.0, 3.1] } ] }";
    let mappings = typed(&json!({"body":["one","two"]}));
    let paths = typed(&json!({"one":"models/one.dmx","two":"models/two.dmx"}));
    let output = model::patch(source, &mappings, &paths).unwrap();
    assert_eq!(
        output.matches("import_scale = 1.0000238418579102").count(),
        2
    );
    assert_eq!(output.matches("custom = { preserve = #[00ff] }").count(), 2);
    assert!(output.contains("{ _class = \"Attachment\" name = \"eyes\" origin = [1.5, 2.0, 3.1] }"));
}
