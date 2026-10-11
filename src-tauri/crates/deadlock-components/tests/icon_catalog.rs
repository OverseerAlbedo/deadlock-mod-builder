use deadlock_components::{heroes, icon_catalog, icons, vector_icons};
use deadlock_core::{CancellationToken, safety};
use deadlock_project::customization::ResourceReplacement;
use deadlock_resource::{Kind,Value,binary};
use deadlock_vpk::{Vpk,write_vpk};
use serde_json::{Value as Json,json};
use std::{collections::BTreeSet,fs,path::{Path,PathBuf}};

fn value(v:&Json)->Value { Value::new(match v {
    Json::String(s)=>Kind::String(s.clone()), Json::Array(a)=>Kind::Array(a.iter().map(value).collect()),
    Json::Object(o)=>Kind::Object(o.iter().map(|(k,v)|(k.clone(),value(v))).collect()),
    _=>Kind::Null,
}) }
fn resource(data:&[u8])->Vec<u8> {
    let mut raw=vec![];
    raw.extend(((28+data.len()) as u32).to_le_bytes());raw.extend(12u16.to_le_bytes());raw.extend(3u16.to_le_bytes());
    raw.extend(8u32.to_le_bytes());raw.extend(1u32.to_le_bytes());raw.extend(b"DATA");raw.extend(8u32.to_le_bytes());
    raw.extend((data.len() as u32).to_le_bytes());raw.extend(data);raw
}
fn definition(v:&Json)->Vec<u8> { resource(&binary::encode(&value(v),[0;16]).unwrap()) }
fn vector(svg:&str)->Vec<u8> {
    let mut data=crc32fast::hash(svg.as_bytes()).to_le_bytes().to_vec();data.extend(0u16.to_le_bytes());data.extend(svg.as_bytes());resource(&data)
}
fn fixture(name:&str)->PathBuf {Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/icons").join(name)}
fn archive(root:&Path,files:Vec<(String,Vec<u8>)>)->Vpk {
    let tree=root.join("source");let mut paths=vec![];
    for (p,b) in files {safety::atomic_write(&tree.join(&p),&b).unwrap();paths.push(p);}
    let pak=root.join("icons_dir.vpk");write_vpk(&tree,&paths,&pak,None,2,&mut ||Ok(())).unwrap();Vpk::open(pak).unwrap()
}

#[test]
fn compiled_game_definitions_find_unnamed_icons_and_hash_variants_without_foreign_heroes() {
    let temp=tempfile::tempdir().unwrap();
    let hd=json!({"hero_future":{"m_strModelName":"models/heroes/future/body.vmdl","m_mapBoundAbilities":{"ESlot_Signature_1":"ability_xyz"},"image":"file://{images}/heroes/unrelated_sm.psd"},
        "hero_other":{"m_strModelName":"models/heroes/other/body.vmdl","image":"file://{images}/heroes/other_sm.psd"}});
    let ad=json!({"ability_xyz":{"m_strAbilityImage":"file://{images}/hud/abilities/unrelated_action.png","secondary":"ability_child"},"ability_child":{"icon":"file://{images}/hud/abilities/child.svg","back":"ability_xyz"}});
    let texture=fs::read(fixture("embedded.vtex_c")).unwrap();
    let paths=["panorama/images/heroes/unrelated_sm_psd.vtex_c","panorama/images/heroes/unrelated_sm_psd_1234abcd.vtex_c","panorama/images/hud/abilities/unrelated_action_png.vtex_c","panorama/images/heroes/other_sm_psd.vtex_c"];
    let mut files=vec![("scripts/heroes.vdata_c".into(),definition(&hd)),("scripts/abilities.vdata_c".into(),definition(&ad))];
    files.extend(paths.iter().map(|p|(p.to_string(),texture.clone())));
    files.push(("panorama/images/hud/abilities/child.vsvg_c".into(),vector("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><path d=\"M0 0L32 32\"/></svg>")));
    let pak=archive(temp.path(),files);let cancel=CancellationToken::new();
    let (rows,warnings)=icons::discover_with_warnings(&pak,"models/heroes/future/body.vmdl",&cancel).unwrap();
    assert!(warnings.is_empty());assert_eq!(rows.len(),4);assert!(rows.iter().all(|r|r.shared));
    assert!(!rows.iter().any(|r|r.target.contains("other_sm")));
    let svg=rows.iter().find(|r|r.target.ends_with(".vsvg_c")).unwrap();assert!(!svg.supported);assert_eq!(svg.method.as_deref(),Some("vector_preview"));assert_eq!((svg.width,svg.height),(32,32));
    let mut replacement=ResourceReplacement {source:Some(fixture("source.png").display().to_string()),target:Some(paths[0].into()),..Default::default()};
    assert!(icons::stage(&pak,&[replacement.clone()],&temp.path().join("staged"),"models/heroes/future/body.vmdl",&cancel).unwrap_err().to_string().contains("shared"));
    replacement.allow_shared=Some(true);
    assert!(icons::stage(&pak,&[replacement],&temp.path().join("staged"),"models/heroes/future/body.vmdl",&cancel).unwrap().entries[0].allow_shared);
}

#[test]
fn legacy_portrait_aliases_are_owned_and_other_heroes_are_excluded() {
    for (model,token) in [("models/heroes_wip/abrams/abrams.vmdl","bull"),("models/heroes_wip/geist/geist.vmdl","spectre"),("models/heroes_wip/dynamo/dynamo.vmdl","sumo"),("models/heroes_staging/viper/viper.vmdl","kali")] {
        let p=format!("panorama/images/heroes/{token}_sm_psd.vtex_c");
        let paths=icon_catalog::select_paths([p.as_str(),"panorama/images/heroes/haze_sm_psd.vtex_c"],model,&BTreeSet::new()).unwrap();
        assert_eq!(paths.len(),1);assert_eq!(paths[&p],false);
        for category in ["sounds/heroes","particles/abilities"] {assert!(heroes::owned(&format!("{category}/{token}/sample"),&heroes::tokens(model).unwrap()));}
    }
}

#[test]
fn shared_model_prototypes_do_not_contaminate_the_named_hero() {
    let doc=value(&json!({"hero_chrono":{"m_strModelName":"models/heroes_staging/chrono/chrono.vmdl","icon":"file://{images}/heroes/chrono_sm.psd"},"hero_gunslinger":{"m_strModelName":"models/heroes_staging/chrono/chrono.vmdl","icon":"file://{images}/heroes/gunslinger_sm.psd"}}));
    let refs=icon_catalog::referenced_images(&doc,None,"models/heroes_staging/chrono/chrono.vmdl").unwrap();
    assert_eq!(refs,BTreeSet::from(["panorama/images/heroes/chrono_sm_psd.vtex_c".to_string()]));
    assert!(icon_catalog::image_path("file://{images}/../../secrets.png").is_none());
}

#[test]
fn malformed_definitions_keep_named_images_and_report_the_incomplete_scan() {
    let temp=tempfile::tempdir().unwrap();let pak=archive(temp.path(),vec![("scripts/heroes.vdata_c".into(),b"broken".to_vec()),("panorama/images/heroes/bull_sm_psd.vtex_c".into(),fs::read(fixture("embedded.vtex_c")).unwrap())]);
    let (rows,warnings)=icons::discover_with_warnings(&pak,"models/heroes_wip/abrams/abrams.vmdl",&CancellationToken::new()).unwrap();
    assert_eq!(rows.len(),1);assert_eq!(warnings.len(),1);
    let cancel=CancellationToken::new();cancel.cancel();assert!(icons::discover(&pak,"models/heroes_wip/abrams/abrams.vmdl",&cancel).is_err());
}

#[test]
fn vector_preview_preserves_artwork_and_rejects_corruption_or_external_content() {
    let svg="<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 16\"><defs><linearGradient id=\"a\"/></defs><path fill=\"url(#a)\" d=\"M0 0L24 16\"/></svg>";
    let mut raw=vector(svg);let p=vector_icons::preview(&raw).unwrap();assert_eq!(p.svg,svg.as_bytes());assert_eq!((p.width,p.height),(24,16));
    *raw.last_mut().unwrap()^=1;assert!(vector_icons::preview(&raw).is_err());
    for content in ["<script>alert(1)</script>","<use href=\"https://example.com/a.svg\"/>","<path onclick=\"alert(1)\"/>","<style>@import 'https://example.com';</style>"] {
        assert!(vector_icons::preview(&vector(&format!("<svg>{content}</svg>"))).is_err());
    }
    if let Some(path)=std::env::var_os("DMB_ICON_NATIVE_FIXTURES") {
        let p=Path::new(&path);assert!(!vector_icons::preview(&fs::read(p.join("pip-left.vsvg_c")).unwrap()).unwrap().svg.is_empty());
        assert!(icon_catalog::definition(&fs::read(p.join("abilities_kv3_v5_zstd.vdata_c")).unwrap()).unwrap().object().unwrap().len()>100);
    }
}

#[test]
fn all_roster_members_include_every_available_referenced_image_in_the_real_game_inventory() {
    let input:Json=serde_json::from_slice(&fs::read(fixture("game-image-inventory.json")).unwrap()).unwrap();
    let hd=value(&input["heroes"]);let ad=value(&input["abilities"]);
    let paths:BTreeSet<String>=input["images"].as_array().unwrap().iter().map(|v|v.as_str().unwrap().to_string()).collect();
    assert_eq!(paths.len(),2742);
    let mut checked=0;
    for hero in heroes::roster().unwrap() {
        let mut models=vec![hero.path];models.extend(hero.extras.into_iter().map(|m|m.path));
        for model in models {
            let refs=icon_catalog::referenced_images(&hd,Some(&ad),&model).unwrap();
            let found=icon_catalog::select_paths(paths.iter().map(String::as_str),&model,&refs).unwrap();
            for path in refs.intersection(&paths) {assert!(found.contains_key(path),"{model} omitted {path}");}
            assert!(!found.is_empty(),"No images shown for {model}");checked+=1;
        }
    }
    assert_eq!(checked,47);
}
