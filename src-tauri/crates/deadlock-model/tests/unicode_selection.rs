//! Compiler selectors must carry literal UTF-8 names, not JSON Unicode escapes.
use deadlock_model::{patch, Mappings};
use deadlock_resource::text::Document;
use std::collections::BTreeMap;
#[test]
fn multilingual_replacements_keep_every_body_lod_and_equipment_reference() {
    for name in ["女管理员(技术测试版)_mesh", "髪 ロング", "Body_é_雪_😀", "hair[long](v2)"] {
        let nodes = ["body", "body_lod1", "body_lod2", "body_lod3"].map(|n|
            format!("{{ _class = \"RenderMeshFile\" name = \"{n}\" filename = \"old.dmx\" }}")).join(", ");
        let refs = ["body", "body_lod1", "body_lod2", "body_lod3"].map(|n|
            format!("{{ _class = \"LODGroup\" mesh_references = [{{ mesh_name = \"{n}\" }}] }}")).join(", ");
        let text = format!("{{ rootNode = {{ children = [{{ _class = \"RenderMeshList\" children = [{nodes}, {{ _class = \"RenderMeshFile\" name = \"weapon\" filename = \"weapon.dmx\" }}] }}, {{ _class = \"LODGroupList\" children = [{refs}] }}] }} }}");
        let mapping: Mappings = ["body", "body_lod1", "body_lod2", "body_lod3"].into_iter()
            .map(|n| (n.into(), deadlock_project::MeshMapping::One(name.into()))).chain([("weapon".into(), deadlock_project::MeshMapping::One("@keep".into()))]).collect();
        let output = patch(&text, &mapping, &BTreeMap::from([(name.into(), "models/replacement.dmx".into())])).unwrap();
        assert!(!output.contains("\\u"));
        let doc = Document::parse(&output).unwrap();
        let nodes = doc.objects(Some("RenderMeshFile"));
        assert_eq!(nodes.len(), 5);
        for node in nodes.into_iter().filter(|n| n.get("name").unwrap().scalar() != Some("weapon")) {
            let selected = node.get("import_filter").unwrap().get("exception_list").unwrap().array().unwrap();
            assert_eq!(selected[0].scalar(), Some(name));
            assert!(output[selected[0].start..selected[0].end].contains(name));
        }
        assert_eq!(doc.objects(Some("LODGroup")).len(), 4);
        assert!(output.contains("filename = \"weapon.dmx\""));
    }
}
