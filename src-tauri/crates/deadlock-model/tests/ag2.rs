use deadlock_model::ag2;
use serde_json::Value;
#[test]
fn authoritative_ag2_source_and_role_python_golden() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/ag2.json")).unwrap();
    for case in cases {
        let a = &case["args"];
        let text = a["text"].as_str().unwrap();
        let result = (|| -> Result<Value, String> {
            let value = match case["operation"].as_str().unwrap() {
                "data" => {
                    serde_json::to_value(ag2::from_data_dump(text).map_err(|e| e.to_string())?)
                        .unwrap()
                }
                "source" => {
                    serde_json::to_value(ag2::source_nodes(text).map_err(|e| e.to_string())?)
                        .unwrap()
                }
                "preview" => {
                    serde_json::to_value(ag2::preview_source(text).map_err(|e| e.to_string())?)
                        .unwrap()
                }
                "compiler" => serde_json::to_value(
                    ag2::compiler_source(
                        text,
                        serde_json::from_value(a["policy"].clone()).unwrap(),
                        serde_json::from_value(a["target"].clone()).unwrap(),
                    )
                    .map_err(|e| e.to_string())?,
                )
                .unwrap(),
                "reconcile" => {
                    let refs: Option<Vec<String>> =
                        serde_json::from_value(a["refs"].clone()).unwrap();
                    serde_json::to_value(
                        ag2::reconcile(
                            text,
                            refs.as_deref(),
                            &serde_json::from_value::<Vec<ag2::Reference>>(a["recovered"].clone())
                                .unwrap(),
                        )
                        .map_err(|e| e.to_string())?,
                    )
                    .unwrap()
                }
                _ => panic!(),
            };
            Ok(value)
        })();
        if case.get("error").is_some() {
            assert!(result.is_err(), "{}", case["name"]);
        } else {
            assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
        }
    }
}
#[test]
fn independent_compiled_role_verification() {
    let expected = vec![ag2::Reference {
        class: "AnimGraph2".into(),
        name: "UI".into(),
        filename: "models/ui.vnmgraph".into(),
    }];
    let refs = vec!["models/ui.vnmgraph_c".into()];
    assert!(
        !ag2::verify(&expected, &refs, None)
            .unwrap()
            .identifiers_verified
    );
    let mut altered = expected.clone();
    altered[0].name = "default".into();
    assert!(ag2::verify(&expected, &refs, Some(&altered)).is_err());
    assert!(
        ag2::verify(&expected, &refs, Some(&expected))
            .unwrap()
            .identifiers_verified
    );
    assert!(ag2::verify(&expected, &[], Some(&expected)).is_err());
}
#[test]
fn native_data_and_cli_preserve_same_graph_roles() {
    use deadlock_resource::{Annotation, Kind as K, Value as V};
    fn s(value: &str) -> V {
        V::new(K::String(value.into()))
    }
    let mut skeleton = s("models/a.vnmskel_c");
    skeleton.annotation = Annotation::Resource;
    let data = V::new(K::Object(vec![
        (
            "m_vecNmSkeletonRefs".into(),
            V::new(K::Array(vec![skeleton])),
        ),
        (
            "m_animGraph2Refs".into(),
            V::new(K::Array(vec![
                V::new(K::Object(vec![
                    ("m_sIdentifier".into(), s("default")),
                    ("m_hGraph".into(), s("models/a.vnmgraph")),
                ])),
                V::new(K::Object(vec![
                    ("m_sIdentifier".into(), s("UI 雪")),
                    ("m_hGraph".into(), s("models/ui.vnmgraph")),
                ])),
            ])),
        ),
    ]));
    let cli = "{m_vecNmSkeletonRefs=[resource:\"models/a.vnmskel_c\"] m_animGraph2Refs=[{m_sIdentifier=\"default\" m_hGraph=resource:\"models/a.vnmgraph\"},{m_sIdentifier=\"UI 雪\" m_hGraph=resource:\"models/ui.vnmgraph\"}]}";
    assert_eq!(
        ag2::from_data(&data).unwrap(),
        ag2::from_data_dump(cli).unwrap()
    );
}
