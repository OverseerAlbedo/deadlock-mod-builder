//! cargo run -p deadlock-components --example audit_icons -- HEROES ABILITIES LIST REPORT
use deadlock_components::{heroes,icon_catalog};
use std::{collections::BTreeSet,fs};

fn main() -> Result<(),Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len()!=5 { return Err("Expected HEROES ABILITIES LIST REPORT paths".into()); }
    let hd=icon_catalog::definition(&fs::read(&args[1])?)?;
    let ad=icon_catalog::definition(&fs::read(&args[2])?)?;
    let listing=fs::read_to_string(&args[3])?;
    let paths:BTreeSet<_>=listing.lines().filter_map(|l|l.split(" CRC:").next()).filter(|p|icon_catalog::image_resource(p)).map(str::to_string).collect();
    let mut rows=vec![];
    for hero in heroes::roster()? {
        let mut members=vec![(hero.name,hero.path)];
        members.extend(hero.extras.into_iter().map(|m|(m.name,m.path)));
        for (name,model) in members {
            let refs=icon_catalog::referenced_images(&hd,Some(&ad),&model)?;
            let selected=icon_catalog::select_paths(paths.iter().map(String::as_str),&model,&refs)?;
            let available:Vec<_>=refs.intersection(&paths).cloned().collect();
            let omitted:Vec<_>=available.iter().filter(|p|!selected.contains_key(*p)).cloned().collect();
            let absent:Vec<_>=refs.difference(&paths).cloned().collect();
            let mut old_tokens=heroes::tokens(&model)?;
            for key in ["bull","spectre","sumo","kali","giga","grappler"] { old_tokens.remove(key); }
            let before=paths.iter().filter(|p|p.ends_with(".vtex_c") && heroes::matches(p,&old_tokens)).count();
            rows.push(serde_json::json!({"hero":name,"model":model,"old_texture_count":before,
                "textures":selected.keys().filter(|p|p.ends_with(".vtex_c")).count(),
                "vectors":selected.keys().filter(|p|p.ends_with(".vsvg_c")).count(),
                "available_referenced_images":available.len(),"omitted_available_references":omitted,
                "references_absent_from_game_index":absent,"paths":selected}));
        }
    }
    let report=serde_json::json!({"panorama_images_in_game_index":paths.len(),"roster_entries":rows.len(),"heroes":rows,
        "scope":"Real game filenames and decompiled definitions. Image payloads and Windows rendering are not certified by this inventory audit."});
    fs::write(&args[4],serde_json::to_string_pretty(&report)?+"\n")?;
    for row in report["heroes"].as_array().unwrap() { println!("{}: {} -> {} textures, {} vectors; {} available references omitted; {} references absent in game index",
        row["hero"],row["old_texture_count"],row["textures"],row["vectors"],row["omitted_available_references"].as_array().unwrap().len(),row["references_absent_from_game_index"].as_array().unwrap().len()); }
    Ok(())
}
