use deadlock_components::{audio, icons, resources::{self,ResourceKind}, service::ComponentService};
use deadlock_core::{CancellationToken, TimingRecorder, process::{Runner,RunnerOptions}};
use deadlock_project::{Project,Settings,customization::ResourceReplacement};
use deadlock_vpk::{Vpk,resource_blocks};
use std::{fs,path::{Path,PathBuf}};
fn fixture(name:&str)->PathBuf{Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)}
fn read(name:&str)->Vec<u8>{fs::read(fixture(name)).unwrap()}
fn wav(rate:u32,channels:u16,values:&[i16])->Vec<u8>{
    let mut out=Vec::new();out.extend(b"RIFF");out.extend((36+values.len() as u32*2).to_le_bytes());out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());out.extend(1u16.to_le_bytes());out.extend(channels.to_le_bytes());out.extend(rate.to_le_bytes());
    out.extend((rate*channels as u32*2).to_le_bytes());out.extend((channels*2).to_le_bytes());out.extend(16u16.to_le_bytes());
    out.extend(b"data");out.extend((values.len() as u32*2).to_le_bytes());for v in values{out.extend(v.to_le_bytes());}out
}
fn values(raw:&[u8])->Vec<i16>{let wave=audio::playable_wave(raw).unwrap();wave[44..].chunks_exact(2).map(|v|i16::from_le_bytes([v[0],v[1]])).collect()}
fn mp3_original()->Vec<u8>{
    let mut out=read("sound-data.bin");let layout=resource_blocks(&out).unwrap();let at=layout.blocks.iter().find(|b|b.name=="DATA").unwrap().offset;
    let payload=read("extras/tone.mp3");out.truncate(layout.declared_size);out[at+2]=2;
    out[at+8..at+12].copy_from_slice(&1280u32.to_le_bytes());out[at+12..at+16].copy_from_slice(&0.16f32.to_le_bytes());
    out[at+28..at+32].copy_from_slice(&(payload.len() as u32).to_le_bytes());out.extend(payload);out
}
#[test]
fn volume_is_baked_into_samples_with_mute_unity_boost_and_clipping(){
    let cancel=CancellationToken::new();let input=[-20000,-10000,-100,0,100,10000,20000,32767];let wave=wav(8000,1,&input);
    for name in ["sound-data.bin","sound-ctrl.bin"]{
        let original=read(name);let source=original.clone();
        for (volume,expected) in [(0,vec![0;8]),(100,input.to_vec()),(200,vec![-32768,-20000,-200,0,200,20000,32767,32767])]{
            let edited=audio::prepare_audio(&original,&wave,"wav",volume,"exact",&cancel).unwrap();
            assert_eq!(values(&edited.resource),expected);assert_eq!(audio::metadata(&edited.resource).unwrap().frames,8);
            audio::validate_sound(&original,&edited.resource).unwrap();assert_eq!(edited.report.volume_percent,volume);
            if volume==200{assert_eq!(edited.report.clipped_samples,3);}
        }
        assert_eq!(original,source);assert!(audio::prepare_audio(&original,&wave,"wav",201,"exact",&cancel).is_err());
    }
}
#[test]
fn automatic_audio_conversion_preserves_timeline_and_never_trims_without_consent(){
    let original=read("sound-data.bin");let cancel=CancellationToken::new();
    let short=wav(8000,1,&[1000,-1000,2000,-2000]);
    let output=audio::prepare_audio(&original,&short,"wav",100,"pad",&cancel).unwrap();
    assert_eq!(values(&output.resource),vec![1000,-1000,2000,-2000,0,0,0,0]);assert_eq!(output.report.padded_frames,4);
    let long=wav(8000,1,&[1000;12]);assert!(audio::prepare_audio(&original,&long,"wav",100,"pad",&cancel).unwrap_err().to_string().contains("Trim longer audio"));
    let output=audio::prepare_audio(&original,&long,"wav",100,"trim",&cancel).unwrap();assert_eq!(output.report.trimmed_frames,4);assert_eq!(values(&output.resource),vec![1000;8]);
    let stereo=wav(16000,2,&[1000,-1000].repeat(16));let output=audio::prepare_audio(&original,&stereo,"wav",150,"pad",&cancel).unwrap();
    assert_eq!(values(&output.resource),vec![0;8]);assert!(output.report.resampled&&output.report.channels_converted);
    assert!(audio::prepare_audio(&original,&stereo,"wav",100,"exact",&cancel).is_err());
}
#[test]
fn mp3_can_be_auditioned_and_replaced_without_changing_loops_or_voice_contract(){
    let original=mp3_original();let cancel=CancellationToken::new();assert_eq!(audio::metadata(&original).unwrap().format,"MP3");
    let preview=audio::preview_audio(&original,&cancel).unwrap();assert_eq!(&preview[..4],b"RIFF");assert_eq!(preview.len(),44+1280*2);
    let source=wav(8000,1,&[1000;1280]);let output=audio::prepare_audio(&original,&source,"wav",175,"pad",&cancel).unwrap();
    let meta=audio::metadata(&output.resource).unwrap();assert_eq!(meta.format,"PCM16");assert_eq!(meta.frames,1280);assert_eq!(meta.loop_start,-1);
    assert_eq!(values(&output.resource),vec![1750;1280]);audio::validate_rendered_sound(&original,&output.resource).unwrap();
    let mut bad=output.resource.clone();let at=resource_blocks(&bad).unwrap().blocks.iter().find(|b|b.name=="DATA").unwrap().offset;
    bad[at+4..at+8].copy_from_slice(&0i32.to_le_bytes());assert!(audio::validate_rendered_sound(&original,&bad).is_err());
    let imported=audio::prepare_audio(&original,&read("extras/tone.mp3"),"mp3",120,"trim",&cancel).unwrap();audio::validate_rendered_sound(&original,&imported.resource).unwrap();
    if let Ok(path)=std::env::var("DMB_EXTRAS_EVIDENCE") {fs::create_dir_all(&path).unwrap();fs::write(Path::new(&path).join("converted.vsnd_c"),output.resource).unwrap();fs::write(Path::new(&path).join("original-mp3.vsnd_c"),original).unwrap();}
}
#[test]
fn preview_decodes_embedded_gpu_and_compressed_textures_without_mutation(){
    let cancel=CancellationToken::new();
    for name in ["embedded.vtex_c","gpu.vtex_c","compressed.vtex_c","one_pixel.vtex_c"]{
        let input=read(&format!("icons/{name}"));let copy=input.clone();let png=icons::preview_texture(&input,64,&cancel).unwrap();let(_,info)=icons::decode_image(&png,&cancel).unwrap();
        assert!(info.width<=64&&info.height<=64);assert_eq!(copy,input);
    }
    // The old custom-mips fixture deliberately omits its small mip. A complete
    // mip chain is previewable, while the truncated resource remains rejected.
    let mut mips=read("icons/custom_mips.vtex_c");let offset=icons::texture_metadata(&mips).unwrap().payload_offset;
    mips.splice(offset..offset,vec![0;32*32*4]);assert!(icons::preview_texture(&mips,64,&cancel).is_ok());
    for name in ["cube.vtex_c","gpu_truncated.vtex_c","bad_lz4.vtex_c","custom_mips.vtex_c"]{assert!(icons::preview_texture(&read(&format!("icons/{name}")),128,&cancel).is_err());}
    cancel.cancel();assert!(icons::preview_texture(&read("icons/gpu.vtex_c"),128,&cancel).is_err());
}
#[test]
fn a_gain_change_invalidates_frozen_build_and_install_and_originals_remain_untouched(){
    let dir=tempfile::tempdir().unwrap();let cancel=CancellationToken::new();let settings=Settings{workspace:dir.path().join("workspace").to_string_lossy().into(),pak:fixture("components_dir.vpk").to_string_lossy().into(),..Default::default()};
    let clock=TimingRecorder::new("extras volume");let runner=Runner::new(dir.path().join("logs"),cancel.clone(),RunnerOptions::default()).unwrap();let log=|_:&str|{};
    let service=ComponentService{settings:&settings,runner:&runner,cancel:&cancel,clock:&clock,log:&log};
    let mut project=Project::default();project.original_model="models/heroes_staging/hornet_v3/hornet.vmdl".into();project.customization.enabled=Some(true);project.customization.model_enabled=Some(false);
    let wave=dir.path().join("voice.wav");let bytes=wav(8000,1,&[1000;8]);fs::write(&wave,&bytes).unwrap();
    project.customization.audio=Some(vec![ResourceReplacement{source:Some(wave.to_string_lossy().into()),target:Some("sounds/heroes/hornet/rifle.vsnd_c".into()),volume_percent:Some(175),audio_fit:Some("pad".into()),..Default::default()}]);
    let output=dir.path().join("game");let receipt=service.compile(&project,&output).unwrap().unwrap();
    assert_eq!(values(&fs::read(output.join("sounds/heroes/hornet/rifle.vsnd_c")).unwrap()),vec![1750;8]);assert_eq!(fs::read(wave).unwrap(),bytes);
    service.verify_snapshot(&project,&receipt,&output).unwrap();let package=dir.path().join("pak19_dir.vpk");deadlock_vpk::write_vpk(&output,&receipt.files.keys().cloned().collect::<Vec<_>>(),&package,Some(&receipt.files),2,&mut||Ok(())).unwrap();
    let candidate=Vpk::open(package).unwrap();let current=Vpk::open(&settings.pak).unwrap();service.verify_install(&project,&receipt,&candidate,&current).unwrap();
    project.customization.audio.as_mut().unwrap()[0].volume_percent=Some(100);
    assert!(service.verify_snapshot(&project,&receipt,&output).is_err());assert!(service.verify_install(&project,&receipt,&candidate,&current).is_err());
}
#[test]
fn discovery_and_shared_ownership_are_preserved_for_the_new_audio_path(){
    let archive=Vpk::open(fixture("components_dir.vpk")).unwrap();let dir=tempfile::tempdir().unwrap();let cancel=CancellationToken::new();
    let choice=ResourceReplacement{source:Some(fixture("samples.wav").to_string_lossy().into()),target:Some("sounds/shared/gun.vsnd_c".into()),volume_percent:Some(200),audio_fit:Some("pad".into()),..Default::default()};
    assert!(resources::stage(&archive,"models/heroes_staging/hornet_v3/hornet.vmdl",&[choice],dir.path(),ResourceKind::Audio,&cancel).is_err());
}

#[test]
fn common_import_formats_and_eight_bit_destinations_use_the_same_editing_path(){
    let original=mp3_original();let cancel=CancellationToken::new();
    for (name,extension) in [("tone.flac","flac"),("tone.ogg","ogg"),("tone-float.wav","wav")]{
        let output=audio::prepare_audio(&original,&read(&format!("extras/{name}")),extension,150,"trim",&cancel).unwrap();
        audio::validate_rendered_sound(&original,&output.resource).unwrap();assert_eq!(values(&output.resource).len(),1280);assert!(values(&output.resource).iter().any(|s|s.abs()>100));assert!(output.report.resampled);
    }
    let mut eight=read("sound-data.bin");let layout=resource_blocks(&eight).unwrap();let at=layout.blocks.iter().find(|b|b.name=="DATA").unwrap().offset;
    eight[at+2]=1;eight[at+28..at+32].copy_from_slice(&8u32.to_le_bytes());eight.truncate(layout.declared_size);eight.extend([0,64,100,128,150,180,200,255]);
    let muted=audio::prepare_audio(&eight,&wav(8000,1,&[1000;8]),"wav",0,"pad",&cancel).unwrap();let metadata=audio::metadata(&muted.resource).unwrap();assert_eq!(metadata.format,"PCM8");assert_eq!(&muted.resource[metadata.payload_offset..],&[128;8]);
}

#[test]
fn ctrl_mp3_conversion_preserves_unrelated_voice_metadata(){
    use deadlock_resource::{binary,container,Kind};
    let raw=read("sound-ctrl.bin");let layout=resource_blocks(&raw).unwrap();let block=layout.blocks.iter().find(|b|b.name=="CTRL").unwrap();
    let mut doc=binary::decode(&raw[block.offset..block.offset+block.size]).unwrap();let sound=doc.root.get_mut("m_vSound").unwrap();let payload=read("extras/tone.mp3");
    sound.get_mut("m_nFormat").unwrap().kind=Kind::String("MP3".into());sound.get_mut("m_nSampleCount").unwrap().kind=Kind::U64(1280);sound.get_mut("m_nStreamingSize").unwrap().kind=Kind::U64(payload.len() as u64);sound.get_mut("m_flDuration").unwrap().kind=Kind::F64(0.16);
    let ctrl=binary::encode(&doc.root,doc.format_guid).unwrap();let mut original=container::rebuild(&raw[..layout.declared_size],&std::collections::BTreeMap::from([("CTRL".into(),ctrl)])).unwrap();original.extend(payload);
    let output=audio::prepare_audio(&original,&wav(8000,1,&[1000;1280]),"wav",175,"pad",&CancellationToken::new()).unwrap();assert_eq!(values(&output.resource),vec![1750;1280]);audio::validate_rendered_sound(&original,&output.resource).unwrap();
    if let Ok(path)=std::env::var("DMB_EXTRAS_EVIDENCE"){fs::write(Path::new(&path).join("converted-ctrl.vsnd_c"),output.resource).unwrap();}
}
