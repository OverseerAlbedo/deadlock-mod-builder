//! A render node named like a bone (GitHub issue 10): fields that name the
//! bone, a physics body or a body group are not references to the render node,
//! while a field that really names the node is still refused.
use deadlock_model::{patch, Mappings, DISABLE, KEEP};
use deadlock_project::MeshMapping;
use deadlock_resource::text::Document;
use std::collections::BTreeMap;

/// Fields that name the bone, physics body or body group `head`.
const BONE_FIELDS: &[&str] = &[
    r#"parent_bone = "head""#,
    r#"bone = "head""#,
    r#"parent_body = "head""#,
    r#"child_body = "head""#,
    r#"target_body = "head""#,
    r#"bodygroup = "head""#,
    r#"m_BodygroupChoice = "head""#,
];

fn model(extra: &str) -> String {
    format!(
        r#"<!-- kv3 encoding:text:version{{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d}} format:modeldoc28:version{{fb63b6ca-f435-4aa0-a2c7-c66ddc651dca}} -->
{{
	rootNode =
	{{
		_class = "RootNode"
		children =
		[
			{{
				_class = "RenderMeshList"
				children =
				[
					{{
						_class = "RenderMeshFile"
						name = "body"
						filename = "models/heroes_wip/sample/sample_body.dmx"
					}},
					{{
						_class = "RenderMeshFile"
						name = "head"
						filename = "models/heroes_wip/sample/sample_head.dmx"
					}},
					{{
						_class = "RenderMeshFile"
						name = "body_lod1"
						filename = "models/heroes_wip/sample/sample_body_lod1.dmx"
					}},
					{{
						_class = "RenderMeshFile"
						name = "head_lod1"
						filename = "models/heroes_wip/sample/sample_head_lod1.dmx"
					}},
				]
			}},
			{{
				_class = "BodyGroupList"
				children =
				[
					{{
						_class = "BodyGroup"
						name = "head"
						children =
						[
							{{
								_class = "BodyGroupChoice"
								name = "head"
								meshes =
								[
									"body",
									"head",
									"body_lod1",
									"head_lod1",
								]
							}},
							{{
								_class = "BodyGroupChoice"
								name = "hidden"
								meshes =
								[
									"body",
									"body_lod1",
								]
							}},
						]
					}},
				]
			}},
			{{
				_class = "LODGroupList"
				children =
				[
					{{
						_class = "LODGroup"
						switch_threshold = 0.0
						mesh_references =
						[
							{{
								mesh_name = "body"
							}},
							{{
								mesh_name = "head"
							}},
						]
					}},
					{{
						_class = "LODGroup"
						switch_threshold = 16.0
						mesh_references =
						[
							{{
								mesh_name = "body_lod1"
							}},
							{{
								mesh_name = "head_lod1"
							}},
						]
					}},
				]
			}},
			{{
				_class = "AttachmentList"
				children =
				[
					{{
						_class = "Attachment"
						name = "eyes"
						parent_bone = "head"
						relative_origin = [ 0.0, 0.0, 0.0 ]
					}},
					{{
						_class = "Attachment"
						name = "hat"
						parent_bone = "head"
						relative_origin = [ 0.0, 0.0, 4.0 ]
					}},
				]
			}},
			{{
				_class = "HitboxSetList"
				children =
				[
					{{
						_class = "HitboxSet"
						name = "default"
						children =
						[
							{{
								_class = "HitboxCapsule"
								name = "head"
								parent_bone = "head"
								radius = 4.0
							}},
						]
					}},
				]
			}},
			{{
				_class = "PhysicsShapeList"
				children =
				[
					{{
						_class = "PhysicsShapeCapsule"
						parent_bone = "head"
						radius = 4.0
					}},
					{{
						_class = "PhysicsJointConical"
						parent_body = "head"
						child_body = "head"
					}},
					{{
						_class = "PhysicsBodyMarkup"
						target_body = "head"
					}},
				]
			}},
			{{
				_class = "WeightListList"
				children =
				[
					{{
						_class = "WeightList"
						name = "head"
						weights =
						[
							{{
								bone = "head"
								weight = 1.0
							}},
						]
					}},
				]
			}},
			{{
				_class = "AnimEvent"
				event_class = "AE_BODYGROUP_SET_VALUE"
				event_keys =
				{{
					bodygroup = "head"
					value = 0
				}}
			}},
			{{
				_class = "GenericGameData"
				game_keys =
				{{
					m_BodygroupChoice = "head"
				}}
			}},
			{{
				_class = "Skeleton"
				children =
				[
					{{
						_class = "Bone"
						name = "head"
						origin = [ 0.0, 0.0, 60.0 ]
					}},
				]
			}},{extra}
		]
	}}
}}
"#
    )
}

fn keep_all() -> Mappings {
    ["body", "head", "body_lod1", "head_lod1"]
        .into_iter()
        .map(|n| (n.to_string(), MeshMapping::One(KEEP.into())))
        .collect()
}

fn disable_head() -> Mappings {
    let mut mappings = keep_all();
    for name in ["head", "head_lod1"] {
        mappings.insert(name.into(), MeshMapping::One(DISABLE.into()));
    }
    mappings
}

fn expand_head() -> (Mappings, BTreeMap<String, String>) {
    let mut mappings = keep_all();
    mappings.insert(
        "head".into(),
        MeshMapping::Many(vec!["face".into(), "hair".into()]),
    );
    let sources = BTreeMap::from([
        ("face".to_string(), "models/replacement/face.dmx".to_string()),
        ("hair".to_string(), "models/replacement/hair.dmx".to_string()),
    ]);
    (mappings, sources)
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

fn assert_bone_fields_kept(before: &str, after: &str) {
    for field in BONE_FIELDS {
        assert!(count(before, field) > 0, "{field} is in the test model");
        assert_eq!(count(after, field), count(before, field), "{field}");
    }
    let doc = Document::parse(after).unwrap();
    let bone = doc.objects(Some("Bone"));
    assert_eq!(bone.len(), 1);
    assert_eq!(bone[0].get("name").unwrap().scalar(), Some("head"));
    let hitbox = doc.objects(Some("HitboxCapsule"));
    assert_eq!(hitbox[0].get("name").unwrap().scalar(), Some("head"));
    assert_eq!(doc.objects(Some("Attachment")).len(), 2);
    let weights = doc.objects(Some("WeightList"));
    assert_eq!(weights[0].get("name").unwrap().scalar(), Some("head"));
}

fn choice_meshes(text: &str, choice: &str) -> Vec<String> {
    let doc = Document::parse(text).unwrap();
    let node = doc
        .objects(Some("BodyGroupChoice"))
        .into_iter()
        .find(|n| n.get("name").unwrap().scalar() == Some(choice))
        .unwrap();
    node.get("meshes")
        .unwrap()
        .array()
        .unwrap()
        .iter()
        .map(|x| x.scalar().unwrap().to_string())
        .collect()
}

fn mesh_references(text: &str) -> Vec<String> {
    let doc = Document::parse(text).unwrap();
    doc.objects(None)
        .into_iter()
        .filter_map(|n| n.get("mesh_name").and_then(|v| v.scalar()).map(str::to_owned))
        .collect()
}

fn render_names(text: &str) -> Vec<String> {
    let doc = Document::parse(text).unwrap();
    doc.objects(Some("RenderMeshFile"))
        .into_iter()
        .map(|n| n.get("name").unwrap().scalar().unwrap().to_string())
        .collect()
}

#[test]
fn disabling_a_render_node_named_like_a_bone_keeps_the_bone_references() {
    let text = model("");
    let output = patch(&text, &disable_head(), &BTreeMap::new()).unwrap();
    assert_eq!(render_names(&output), ["body", "body_lod1"]);
    assert_eq!(mesh_references(&output), ["body", "body_lod1"]);
    assert_eq!(choice_meshes(&output, "head"), ["body", "body_lod1"]);
    assert_eq!(choice_meshes(&output, "hidden"), ["body", "body_lod1"]);
    assert_bone_fields_kept(&text, &output);
}

#[test]
fn expanding_a_render_node_named_like_a_bone_keeps_the_bone_references() {
    let text = model("");
    let (mappings, sources) = expand_head();
    let output = patch(&text, &mappings, &sources).unwrap();
    assert_eq!(
        render_names(&output),
        ["body", "head__part01", "head__part02", "body_lod1", "head_lod1"]
    );
    assert_eq!(
        mesh_references(&output),
        ["body", "head__part01", "head__part02", "body_lod1", "head_lod1"]
    );
    assert_eq!(
        choice_meshes(&output, "head"),
        ["body", "head__part01", "head__part02", "body_lod1", "head_lod1"]
    );
    assert!(output.contains(r#"filename = "models/replacement/face.dmx""#));
    assert!(output.contains(r#"filename = "models/replacement/hair.dmx""#));
    assert_bone_fields_kept(&text, &output);
}

const UNKNOWN_SCALAR: &str = r#"
			{
				_class = "SampleUnknownNode"
				mesh_link = "head"
			},"#;
const UNKNOWN_ARRAY: &str = r#"
			{
				_class = "SampleUnknownNode"
				mesh_links = [ "head" ]
			},"#;
const UNKNOWN_NESTED_SCALAR: &str = r#"
			{
				_class = "Attachment"
				name = "sample"
				parent_bone = "head"
				settings =
				{
					mesh_link = "head"
				}
			},"#;

fn refusal(result: deadlock_model::Result<String>) -> String {
    result.expect_err("refused").to_string()
}

#[test]
fn an_unknown_scalar_reference_to_a_disabled_node_is_still_refused() {
    for extra in [UNKNOWN_SCALAR, UNKNOWN_NESTED_SCALAR] {
        let text = model(extra);
        assert_eq!(
            refusal(patch(&text, &disable_head(), &BTreeMap::new())),
            "Disabling head leaves an unsupported mesh_link reference; keep it or use a custom source."
        );
    }
}

#[test]
fn an_unknown_array_reference_to_a_disabled_node_is_still_refused() {
    let text = model(UNKNOWN_ARRAY);
    assert_eq!(
        refusal(patch(&text, &disable_head(), &BTreeMap::new())),
        "Disabled mesh remains referenced in mesh_links; keep it or use a custom source."
    );
}

#[test]
fn an_unknown_scalar_reference_to_an_expanded_node_is_still_refused() {
    let (mappings, sources) = expand_head();
    for extra in [UNKNOWN_SCALAR, UNKNOWN_NESTED_SCALAR] {
        let text = model(extra);
        assert_eq!(
            refusal(patch(&text, &mappings, &sources)),
            "Cannot safely expand unknown mesh_link reference to head."
        );
    }
}

#[test]
fn an_unknown_array_reference_to_an_expanded_node_is_still_refused() {
    let (mappings, sources) = expand_head();
    let text = model(UNKNOWN_ARRAY);
    assert_eq!(
        refusal(patch(&text, &mappings, &sources)),
        "Cannot safely expand mesh references in mesh_links."
    );
}

#[test]
fn keeping_every_node_leaves_the_model_unchanged() {
    let text = model("");
    assert_eq!(patch(&text, &keep_all(), &BTreeMap::new()).unwrap(), text);
}
