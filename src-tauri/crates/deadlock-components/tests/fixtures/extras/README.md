# Synthetic Extras audio fixtures

These tones were generated for this test suite; they are not game/user audio.

MP3: FFmpeg lavfi `sine=frequency=400:duration=0.16`, mono 8000 Hz,
libmp3lame 24k, `-write_xing 0 -id3v2_version 0`.
FLAC / OGG / float WAV: the same sine, mono 16000 Hz, respectively codecs
`flac`, `libvorbis`, `pcm_f32le`. All are imported through the production decoder.
