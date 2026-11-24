use deadlock_resource::{Annotation, Kind, Value, binary};

const FOOTER: u32 = 0xffeedd00;

// Independently laid out v4 lanes, annotations, sizes and compression frames.
fn fixture(method: u32, blobs: &[Vec<u8>], joined_zstd: bool) -> (Vec<u8>, Value) {
    fixture_table(method, blobs, joined_zstd, true)
}
/// `stated`: whether the header states the size of the LZ4 frame table (some compiled
/// physics blocks store the table after the footer and state its size as 0).
fn fixture_table(method: u32, blobs: &[Vec<u8>], joined_zstd: bool, stated: bool) -> (Vec<u8>, Value) {
    let mut body = Vec::new();
    body.extend((-2i16).to_le_bytes()); body.extend(u16::MAX.to_le_bytes());
    for n in [1, 4 + blobs.len() as u32, 0.1f32.to_bits(), 0] { body.extend(n.to_le_bytes()); }
    body.resize(body.len().div_ceil(8) * 8, 0);
    let mut types = b"sounds/test.vsnd\0".to_vec();
    types.extend([8, 20, 21, 19, 0x86, 1]);
    types.extend(vec![7; blobs.len()]);
    body.extend(&types);
    for blob in blobs { body.extend((blob.len() as u32).to_le_bytes()); }
    body.extend(FOOTER.to_le_bytes());
    let joined: Vec<u8> = blobs.iter().flatten().copied().collect();
    let mut blob_packed = Vec::new();
    let mut history = Vec::new();
    let mut frames = Vec::new();
    if method == 1 {
        for blob in blobs {
            for chunk in blob.chunks(16384) {
                let packed = lz4_flex::block::compress_with_dict(chunk, &history[history.len().saturating_sub(65536)..]);
                frames.extend((packed.len() as u16).to_le_bytes());
                blob_packed.extend(packed); history.extend(chunk);
            }
        }
    }
    body.extend(&frames);
    let packed = match method {
        0 => body.clone(),
        1 => lz4_flex::block::compress(&body),
        2 if joined_zstd => zstd::bulk::compress(&[body.as_slice(), &joined].concat(), 1).unwrap(),
        2 => {
            let mut packed = zstd::bulk::compress(&body, 1).unwrap();
            if !blobs.is_empty() { packed.extend(zstd::bulk::compress(&joined, 1).unwrap()); }
            packed
        }
        _ => unreachable!(),
    };
    let mut out = b"\x043VK".to_vec(); out.extend([0; 16]);
    out.extend(method.to_le_bytes()); out.extend(0u16.to_le_bytes());
    out.extend((if method == 1 { 16384u16 } else { 0 }).to_le_bytes());
    for n in [0, 4, 0, types.len() as u32] { out.extend(n.to_le_bytes()); }
    out.extend(0u16.to_le_bytes()); out.extend(1u16.to_le_bytes());
    for n in [body.len(), packed.len(), blobs.len(), joined.len(), 2, if stated { frames.len() } else { 0 }] { out.extend((n as u32).to_le_bytes()); }
    out.extend(packed);
    if method == 0 { out.extend(&joined); } else if method == 1 { out.extend(blob_packed); }
    if !blobs.is_empty() { out.extend(FOOTER.to_le_bytes()); }
    let mut expected = vec![Value::new(Kind::I64(-2)), Value::new(Kind::I64(65535)), Value::new(Kind::F32(0.1)),
        Value { annotation: Annotation::Resource, kind: Kind::String("sounds/test.vsnd".into()) }];
    expected.extend(blobs.iter().map(|b| Value::new(Kind::Blob(b.clone()))));
    (out, Value::new(Kind::Array(expected)))
}

#[test]
fn v4_preserves_typed_lanes_annotations_empty_blobs_and_chained_compression() {
    let patterned: Vec<u8> = (0..35000).map(|i| (i % 251) as u8).collect();
    for blobs in [vec![], vec![vec![], vec![]], vec![patterned.clone(), vec![], patterned[..10000].to_vec()]] {
        for (method, joined) in [(0, false), (1, false), (2, false), (2, true)] {
            let (raw, expected) = fixture(method, &blobs, joined);
            let document = binary::decode(&raw).unwrap();
            assert_eq!(document.version, 4);
            assert!(document.root.canonical_eq(&expected));
            let encoded = binary::encode(&document.root, document.format_guid).unwrap();
            assert!(binary::decode(&encoded).unwrap().root.canonical_eq(&expected));
        }
    }
}

#[test]
fn v4_reads_an_lz4_frame_table_whose_size_is_stated_as_zero() {
    let patterned: Vec<u8> = (0..35000).map(|i| (i % 251) as u8).collect();
    for blobs in [vec![vec![1, 2, 3]], vec![patterned.clone(), vec![], patterned[..10000].to_vec()], vec![vec![7; 20]; 7]] {
        let (raw, expected) = fixture_table(1, &blobs, false, false);
        let document = binary::decode(&raw).unwrap();
        assert!(document.root.canonical_eq(&expected));
        assert!(binary::unstated_frame_table(&raw));
        // The same document as with the size stated.
        let (stated, _) = fixture_table(1, &blobs, false, true);
        assert!(!binary::unstated_frame_table(&stated));
        assert!(binary::decode(&stated).unwrap().root.canonical_eq(&document.root));
        for end in 0..raw.len() {
            assert!(binary::decode(&raw[..end]).is_err(), "truncated at {end}");
        }
    }
}

#[test]
fn v4_refuses_an_unstated_frame_table_that_does_not_fit_the_blobs() {
    let blobs = vec![vec![1, 2, 3], vec![4; 40]];
    // Without blobs, or without LZ4, nothing may follow the footer.
    for method in [0, 2] {
        let (raw, _) = fixture_table(method, &blobs, false, false);
        assert!(binary::decode(&raw).is_ok(), "method {method}: no table to state");
        assert!(!binary::unstated_frame_table(&raw));
    }
    // A table entry too many, too few, or a stray byte after the footer: refused.
    let (raw, _) = fixture_table(1, &blobs, false, false);
    let document = binary::decode(&raw).unwrap();
    assert_eq!(document.version, 4);
    let unpack = |raw: &[u8]| {
        let size = u32::from_le_bytes(raw[48..52].try_into().unwrap()) as usize;
        let packed = u32::from_le_bytes(raw[52..56].try_into().unwrap()) as usize;
        (lz4_flex::block::decompress(&raw[72..72 + packed], size).unwrap(), packed)
    };
    let repack = |raw: &[u8], body: &[u8], packed: usize| {
        let again = lz4_flex::block::compress(body);
        let mut out = raw[..72].to_vec();
        out[48..52].copy_from_slice(&(body.len() as u32).to_le_bytes());
        out[52..56].copy_from_slice(&(again.len() as u32).to_le_bytes());
        out.extend(again);
        out.extend(&raw[72 + packed..]);
        out
    };
    let (body, packed) = unpack(&raw);
    for change in [
        [body.as_slice(), &[5, 0]].concat(),     // an entry no blob uses
        body[..body.len() - 2].to_vec(),         // an entry missing
        [body.as_slice(), &[0]].concat(),        // an odd byte
    ] {
        assert!(binary::decode(&repack(&raw, &change, packed)).is_err());
    }
    // The unchanged body repacked still reads (the repacking itself is sound).
    assert!(binary::decode(&repack(&raw, &body, packed)).is_ok());
}

#[test]
fn v4_rejects_bad_lengths_truncation_extra_frames_and_missing_empty_blob_footer() {
    for method in [0, 1, 2] {
        let (good, _) = fixture(method, &[vec![], vec![1, 2, 3]], false);
        for end in 0..good.len() { assert!(binary::decode(&good[..end]).is_err(), "method {method} truncated at {end}"); }
        let mut bad = good.clone(); bad.push(0); assert!(binary::decode(&bad).is_err());
        let mut bad = good.clone(); *bad.last_mut().unwrap() ^= 1; assert!(binary::decode(&bad).is_err());
        for offset in [28, 32, 36, 40, 48, 52, 56, 60, 64, 68] {
            let mut bad = good.clone(); bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(binary::decode(&bad).is_err(), "method {method} header at {offset}");
        }
        let (mut empty, _) = fixture(method, &[vec![]], false);
        empty.truncate(empty.len() - 4); assert!(binary::decode(&empty).is_err());
    }
    let (mut extra, _) = fixture(2, &[vec![1, 2, 3]], false);
    let frame = zstd::bulk::compress(b"", 1).unwrap();
    let at = extra.len() - 4; extra.splice(at..at, frame.clone());
    let packed = u32::from_le_bytes(extra[52..56].try_into().unwrap()) + frame.len() as u32;
    extra[52..56].copy_from_slice(&packed.to_le_bytes());
    assert!(binary::decode(&extra).is_err());
}
