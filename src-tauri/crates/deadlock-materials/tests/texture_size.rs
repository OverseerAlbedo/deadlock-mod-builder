use deadlock_core::{safety,CancellationToken};
use deadlock_materials::{generate,normalize_staged_textures,plan_cache,texture_references,Plans};
use image::{DynamicImage,ImageBuffer,Rgba,ImageFormat,ImageReader,ImageDecoder,ImageEncoder};
use std::{fs,io::Cursor};

#[test]
fn reported_1181_texture_is_resized_only_in_build_copy_and_changes_cache_key() {
    let temp=tempfile::tempdir().unwrap();let source=temp.path().join("texture.png");
    image::RgbaImage::from_pixel(1181,1181,Rgba([37,91,172,119])).save(&source).unwrap();
    let before=safety::digest(&source).unwrap();let content=temp.path().join("content");
    let material="models/test/body.vmat".to_string();
    let plans:Plans=serde_json::from_value(serde_json::json!({&material:{"confirmed":true,"textures":{"color":source}}})).unwrap();
    generate(&plans,&content,"models/test",&CancellationToken::new()).unwrap();
    let original_vmat=fs::read_to_string(content.join(&material)).unwrap();
    let path=content.join(&texture_references(&original_vmat)[0]);
    let compiler=temp.path().join("compiler.exe");fs::write(&compiler,"compiler fixture").unwrap();
    let cache_before=plan_cache(&material,&content,&compiler,temp.path(),&CancellationToken::new()).unwrap();
    let receipt=normalize_staged_textures(&content,&[material.clone()],&CancellationToken::new()).unwrap();
    assert_eq!(receipt.resized.len(),1);assert_eq!(receipt.resized[0].original_dimensions,[1181,1181]);
    assert_eq!(receipt.resized[0].dimensions,[2048,2048]);
    assert_eq!(safety::digest(&source).unwrap(),before);
    assert_eq!(fs::read_to_string(content.join(&material)).unwrap(),original_vmat);
    let decoded=image::open(&path).unwrap().to_rgba8();assert_eq!(decoded.dimensions(),(2048,2048));
    assert!(decoded.pixels().all(|p|p.0==[37,91,172,119]));
    let cache_after=plan_cache(&material,&content,&compiler,temp.path(),&CancellationToken::new()).unwrap();
    assert_ne!(cache_before.key,cache_after.key);
    let normalized=fs::read(&path).unwrap();
    assert!(normalize_staged_textures(&content,&[material],&CancellationToken::new()).unwrap().resized.is_empty());
    assert_eq!(fs::read(path).unwrap(),normalized);
}

#[test]
fn all_supported_texture_formats_and_nested_vtex_dependencies_use_the_full_uv_grid() {
    for (extension,format) in [("png",ImageFormat::Png),("jpg",ImageFormat::Jpeg),("tga",ImageFormat::Tga),("tif",ImageFormat::Tiff),("exr",ImageFormat::OpenExr)] {
        let temp=tempfile::tempdir().unwrap();let path=temp.path().join(format!("map.{extension}"));
        let image=if format==ImageFormat::OpenExr {
            DynamicImage::ImageRgb32F(ImageBuffer::from_pixel(17,9,image::Rgb([2.,0.5,0.25])))
        } else if format==ImageFormat::Jpeg {
            DynamicImage::ImageRgb8(ImageBuffer::from_pixel(17,9,image::Rgb([90,120,150])))
        } else { DynamicImage::ImageRgba8(ImageBuffer::from_pixel(17,9,Rgba([90,120,150,100]))) };
        image.save_with_format(&path,format).unwrap();
        fs::write(temp.path().join("material.vmat"),"Layer0 { TextureColor1 \"MAP.VTEX\" TextureNormal1 \"MAP.VTEX\" }").unwrap();
        fs::write(temp.path().join("MAP.VTEX"),format!("{{ input_file \"map.{extension}\" default \"engine/missing.vtex\" }}")).unwrap();
        let r=normalize_staged_textures(temp.path(),&["material.vmat".into()],&CancellationToken::new()).unwrap();
        assert_eq!(r.checked_textures,1);assert_eq!(r.resized[0].dimensions,[32,16],"{extension}");
        assert_eq!(ImageReader::open(&path).unwrap().with_guessed_format().unwrap().into_dimensions().unwrap(),(32,16));
        if format==ImageFormat::OpenExr { assert!(image::open(path).unwrap().to_rgb32f().pixels().all(|p|(p[0]-2.).abs()<1e-5)); }
    }
}

#[test]
fn png_and_tiff_keep_sixteen_bit_precision_and_embedded_color_profile() {
    for format in [ImageFormat::Png,ImageFormat::Tiff] {
        let temp=tempfile::tempdir().unwrap();let extension=if format==ImageFormat::Png {"png"}else{"tif"};
        let pixels=DynamicImage::ImageRgba16(ImageBuffer::from_pixel(3,5,Rgba([12345,45678,65535,23456])));
        let mut raw=vec![];let icc=vec![7u8;128];
        if format==ImageFormat::Png {
            let mut encoder=image::codecs::png::PngEncoder::new(&mut raw);encoder.set_icc_profile(icc.clone()).unwrap();
            encoder.write_image(pixels.as_bytes(),3,5,pixels.color().into()).unwrap();
        } else {
            let mut encoder=image::codecs::tiff::TiffEncoder::new(Cursor::new(&mut raw));encoder.set_icc_profile(icc.clone()).unwrap();
            encoder.write_image(pixels.as_bytes(),3,5,pixels.color().into()).unwrap();
        }
        let path=temp.path().join(format!("map.{extension}"));fs::write(&path,raw).unwrap();
        let profile=|path:&std::path::Path| if format==ImageFormat::Tiff {
            tiff::decoder::Decoder::new(Cursor::new(fs::read(path).unwrap())).unwrap().find_tag_unsigned_vec::<u8>(tiff::tags::Tag::IccProfile).unwrap()
        } else { ImageReader::open(path).unwrap().into_decoder().unwrap().icc_profile().unwrap() };
        assert_eq!(profile(&path),Some(icc.clone()),"source {extension}");
        fs::write(temp.path().join("m.vmat"),format!("TextureColor1 \"map.{extension}\"")).unwrap();
        normalize_staged_textures(temp.path(),&["m.vmat".into()],&CancellationToken::new()).unwrap();
        let decoder=ImageReader::open(&path).unwrap().into_decoder().unwrap();
        assert_eq!(profile(&path),Some(icc),"{extension}");
        let output=DynamicImage::from_decoder(decoder).unwrap();assert_eq!(output.color(),image::ColorType::Rgba16);
        assert!(output.to_rgba16().pixels().all(|p|p.0==[12345,45678,65535,23456]));
    }
}

#[test]
fn power_of_two_files_are_byte_preserved_and_cancelled_or_invalid_sources_are_not_written() {
    let temp=tempfile::tempdir().unwrap();let path=temp.path().join("map.png");
    image::RgbaImage::from_pixel(32,16,Rgba([1,2,3,4])).save(&path).unwrap();
    fs::write(temp.path().join("m.vmat"),"TextureColor1 \"map.png\"").unwrap();let raw=fs::read(&path).unwrap();
    assert!(normalize_staged_textures(temp.path(),&["m.vmat".into()],&CancellationToken::new()).unwrap().resized.is_empty());
    assert_eq!(fs::read(&path).unwrap(),raw);
    let cancel=CancellationToken::new();cancel.cancel();assert!(normalize_staged_textures(temp.path(),&["m.vmat".into()],&cancel).is_err());
    fs::write(&path,"broken").unwrap();
    assert!(normalize_staged_textures(temp.path(),&["m.vmat".into()],&CancellationToken::new()).unwrap_err().to_string().contains("map.png"));
    assert_eq!(fs::read(path).unwrap(),b"broken");
}
