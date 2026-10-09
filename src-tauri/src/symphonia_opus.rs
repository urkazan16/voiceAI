//! Strict experimental Ogg/Opus backend. This module never starts a process,
//! writes an intermediate WAV, or falls back to the legacy converter.

use crate::error::{LfError, LfResult};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use symphonia::core::audio::{Audio, GenericAudioBufferRef};
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia_adapter_libopus::OpusDecoder;

pub const BACKEND_NAME: &str = "symphonia-libopus";
pub const DEFAULT_MAX_INPUT_BYTES: u64 = 80 * 1024 * 1024;
pub const DEFAULT_MAX_OUTPUT_SAMPLES: usize = 16_000 * 60 * 60;
const MAX_OGG_PACKET_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct DecodeOptions {
    pub max_input_bytes: u64,
    pub max_output_samples: usize,
    pub deadline: Option<Instant>,
    pub cancellation: Arc<AtomicBool>,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_output_samples: DEFAULT_MAX_OUTPUT_SAMPLES,
            deadline: None,
            cancellation: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DecodeOptions {
    fn checkpoint(&self) -> LfResult<()> {
        if self.cancellation.load(Ordering::Relaxed) {
            return Err(LfError::AudioCancelled(
                "Audio decoding was cancelled.".into(),
            ));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(LfError::AudioDecodeTimeout(
                "Audio decoding timed out.".into(),
            ));
        }
        Ok(())
    }
}

pub fn decode_path(path: &Path, options: &DecodeOptions) -> LfResult<Vec<f32>> {
    options.checkpoint()?;
    let metadata = std::fs::metadata(path)?;
    validate_input_size(metadata.len(), options)?;
    if metadata.len() == 0 {
        return Err(invalid_input());
    }
    validate_ogg(File::open(path)?, options)?;
    options.checkpoint()?;
    decode_source(Box::new(File::open(path)?), options, None)
}

pub fn decode_bytes(bytes: &[u8], options: &DecodeOptions) -> LfResult<Vec<f32>> {
    validate_input_size(bytes.len() as u64, options)?;
    if bytes.is_empty() {
        return Err(invalid_input());
    }
    validate_ogg(Cursor::new(bytes), options)?;
    options.checkpoint()?;
    decode_source(Box::new(Cursor::new(bytes)), options, None)
}

fn validate_input_size(len: u64, options: &DecodeOptions) -> LfResult<()> {
    if len > options.max_input_bytes {
        return Err(LfError::AudioLimitExceeded(format!(
            "Audio input exceeds the {} byte limit.",
            options.max_input_bytes
        )));
    }
    Ok(())
}

fn decode_source<'a>(
    source: Box<dyn MediaSource + 'a>,
    options: &DecodeOptions,
    mut decoded_48k: Option<&mut Vec<f32>>,
) -> LfResult<Vec<f32>> {
    let stream = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(map_probe_error)?;

    if format.tracks().len() != 1 {
        return Err(unsupported_profile());
    }
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| LfError::AudioFormatUnsupported("No Opus audio track was found.".into()))?;
    let params = match track.codec_params.as_ref() {
        Some(CodecParameters::Audio(params)) if params.codec == CODEC_ID_OPUS => params.clone(),
        Some(CodecParameters::Audio(_)) => {
            return Err(LfError::AudioFormatUnsupported(
                "The Ogg stream does not contain Opus audio.".into(),
            ))
        }
        _ => return Err(invalid_input()),
    };
    let track_id = track.id;

    let mut codecs = CodecRegistry::new();
    codecs.register_audio_decoder::<OpusDecoder>();
    let mut decoder = codecs
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(map_decoder_setup_error)?;
    let mut resampler = FirResampler48To16::new();
    let mut output = Vec::new();

    loop {
        options.checkpoint()?;
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => return Err(unsupported_profile()),
            Err(err) => return Err(map_decode_error(err)),
        };
        if packet.track_id != track_id {
            return Err(unsupported_profile());
        }
        let decoded = decoder.decode(&packet).map_err(map_decode_error)?;
        let buffer = match decoded {
            GenericAudioBufferRef::F32(buffer) => buffer,
            _ => {
                return Err(LfError::AudioOutputInvalid(
                    "The Opus decoder returned an unexpected sample format.".into(),
                ))
            }
        };
        let channels = buffer.spec().channels().count();
        match channels {
            1 => {
                let mono = buffer.plane(0).ok_or_else(invalid_output)?;
                if let Some(capture) = decoded_48k.as_deref_mut() {
                    capture.extend_from_slice(mono);
                }
                resampler.push(mono, &mut output, options)?;
            }
            2 => {
                let (left, right) = buffer.plane_pair(0, 1).ok_or_else(invalid_output)?;
                let frames = left.len().min(right.len());
                let mut mono = Vec::new();
                mono.try_reserve_exact(frames).map_err(|_| {
                    LfError::AudioLimitExceeded("Unable to allocate the downmix buffer.".into())
                })?;
                mono.extend(
                    left[..frames]
                        .iter()
                        .zip(&right[..frames])
                        .map(|(l, r)| (*l + *r) * 0.5),
                );
                if let Some(capture) = decoded_48k.as_deref_mut() {
                    capture.extend_from_slice(&mono);
                }
                resampler.push(&mono, &mut output, options)?;
            }
            _ => {
                return Err(LfError::AudioChannelsUnsupported(
                    "Этот вариант аудиофайла пока не поддерживается".into(),
                ))
            }
        }
    }
    resampler.finish(&mut output, options)?;
    if output.is_empty() || output.iter().any(|sample| !sample.is_finite()) {
        return Err(invalid_output());
    }
    Ok(output)
}

fn check_output_budget(current: usize, additional: usize, options: &DecodeOptions) -> LfResult<()> {
    let total = current.checked_add(additional).ok_or_else(|| {
        LfError::AudioLimitExceeded("Decoded audio sample count overflowed.".into())
    })?;
    if total > options.max_output_samples {
        return Err(LfError::AudioLimitExceeded(format!(
            "Decoded audio exceeds the {} sample limit.",
            options.max_output_samples
        )));
    }
    Ok(())
}

fn map_probe_error(error: SymphoniaError) -> LfError {
    match error {
        SymphoniaError::Unsupported(_) => {
            LfError::AudioFormatUnsupported("Этот вариант аудиофайла пока не поддерживается".into())
        }
        SymphoniaError::LimitError(_) => {
            LfError::AudioLimitExceeded("Audio container limit exceeded.".into())
        }
        _ => invalid_input(),
    }
}

fn map_decoder_setup_error(error: SymphoniaError) -> LfError {
    match error {
        SymphoniaError::Unsupported(message) if message.contains("channels") => {
            LfError::AudioChannelsUnsupported(
                "Этот вариант аудиофайла пока не поддерживается".into(),
            )
        }
        SymphoniaError::Unsupported(_) => unsupported_profile(),
        _ => invalid_input(),
    }
}

fn map_decode_error(error: SymphoniaError) -> LfError {
    match error {
        SymphoniaError::Unsupported(_) | SymphoniaError::ResetRequired => unsupported_profile(),
        SymphoniaError::LimitError(_) => {
            LfError::AudioLimitExceeded("Audio decoder limit exceeded.".into())
        }
        _ => LfError::AudioDecodeFailed(
            "Не удалось прочитать аудиофайл. Возможно, он повреждён".into(),
        ),
    }
}

fn invalid_input() -> LfError {
    LfError::AudioInputInvalid("Не удалось прочитать аудиофайл. Возможно, он повреждён".into())
}

fn invalid_output() -> LfError {
    LfError::AudioOutputInvalid("The decoder produced no usable audio.".into())
}

fn unsupported_profile() -> LfError {
    LfError::AudioFormatUnsupported("Этот вариант аудиофайла пока не поддерживается".into())
}

#[derive(Debug)]
struct OggValidation {
    serial: Option<u32>,
    next_sequence: u32,
    saw_eos: bool,
    packet_index: usize,
    packet_len: usize,
    header_packet: Vec<u8>,
    previous_page_continued: bool,
    last_audio_granule: Option<u64>,
}

fn validate_ogg(mut reader: impl Read, options: &DecodeOptions) -> LfResult<()> {
    let mut state = OggValidation {
        serial: None,
        next_sequence: 0,
        saw_eos: false,
        packet_index: 0,
        packet_len: 0,
        header_packet: Vec::new(),
        previous_page_continued: false,
        last_audio_granule: None,
    };
    loop {
        options.checkpoint()?;
        let mut header = [0_u8; 27];
        match read_exact_or_eof(&mut reader, &mut header) {
            Ok(false) => break,
            Ok(true) => {}
            Err(_) => return Err(invalid_input()),
        }
        if state.saw_eos {
            return Err(unsupported_profile());
        }
        if &header[..4] != b"OggS" || header[4] != 0 || header[5] & !0x07 != 0 {
            return Err(invalid_input());
        }
        let flags = header[5];
        let granule = u64::from_le_bytes(header[6..14].try_into().unwrap());
        let serial = u32::from_le_bytes(header[14..18].try_into().unwrap());
        let sequence = u32::from_le_bytes(header[18..22].try_into().unwrap());
        let expected_crc = u32::from_le_bytes(header[22..26].try_into().unwrap());
        let first_page = state.serial.is_none();
        if first_page {
            if flags & 0x03 != 0x02 || sequence != 0 || granule != 0 {
                return Err(invalid_input());
            }
            state.serial = Some(serial);
        } else if state.serial != Some(serial)
            || flags & 0x02 != 0
            || sequence != state.next_sequence
        {
            return Err(unsupported_profile());
        } else if (flags & 0x01 != 0) != state.previous_page_continued {
            return Err(invalid_input());
        }
        state.next_sequence = sequence.checked_add(1).ok_or_else(invalid_input)?;

        let segment_count = usize::from(header[26]);
        let mut lacing = vec![0_u8; segment_count];
        reader
            .read_exact(&mut lacing)
            .map_err(|_| invalid_input())?;
        let body_len = lacing.iter().try_fold(0_usize, |sum, &value| {
            sum.checked_add(usize::from(value))
                .ok_or_else(invalid_input)
        })?;
        let mut body = vec![0_u8; body_len];
        reader.read_exact(&mut body).map_err(|_| invalid_input())?;

        header[22..26].fill(0);
        let mut crc = 0_u32;
        crc = ogg_crc(crc, &header);
        crc = ogg_crc(crc, &lacing);
        crc = ogg_crc(crc, &body);
        if crc != expected_crc {
            return Err(invalid_input());
        }

        let packet_index_before = state.packet_index;
        let mut completed_comment = false;
        let mut completed_audio = false;
        let mut offset = 0_usize;
        for segment in &lacing {
            let len = usize::from(*segment);
            let end = offset.checked_add(len).ok_or_else(invalid_input)?;
            let data = body.get(offset..end).ok_or_else(invalid_input)?;
            state.packet_len = state
                .packet_len
                .checked_add(len)
                .ok_or_else(invalid_input)?;
            if state.packet_len > MAX_OGG_PACKET_BYTES {
                return Err(LfError::AudioLimitExceeded(
                    "An Ogg packet exceeds the prototype limit.".into(),
                ));
            }
            if state.packet_index < 2 {
                state.header_packet.try_reserve(data.len()).map_err(|_| {
                    LfError::AudioLimitExceeded("Unable to allocate the Ogg header buffer.".into())
                })?;
                state.header_packet.extend_from_slice(data);
            }
            if *segment < 255 {
                if state.packet_index >= 2 && state.packet_len == 0 {
                    return Err(invalid_input());
                }
                if state.packet_index == 0 {
                    validate_opus_header(&state.header_packet)?;
                    state.header_packet.clear();
                } else if state.packet_index == 1 {
                    validate_opus_tags(&state.header_packet)?;
                    state.header_packet.clear();
                    completed_comment = true;
                } else {
                    completed_audio = true;
                }
                state.packet_index += 1;
                state.packet_len = 0;
            }
            offset = end;
        }
        let ends_continued = lacing.last() == Some(&255);
        if first_page && state.packet_index != 1 {
            return Err(invalid_input());
        }
        if completed_comment {
            if granule != 0 || state.packet_index != 2 || ends_continued {
                return Err(invalid_input());
            }
        } else if packet_index_before <= 1 && !first_page && granule != u64::MAX {
            return Err(invalid_input());
        }
        if completed_audio {
            if granule == u64::MAX
                || state
                    .last_audio_granule
                    .is_some_and(|previous| granule < previous)
            {
                return Err(invalid_input());
            }
            state.last_audio_granule = Some(granule);
        } else if packet_index_before >= 2 && granule != u64::MAX {
            return Err(invalid_input());
        }
        if flags & 0x04 != 0 && (ends_continued || !completed_audio) {
            return Err(invalid_input());
        }
        state.previous_page_continued = ends_continued;
        state.saw_eos = flags & 0x04 != 0;
    }
    if !state.saw_eos || state.packet_len != 0 || state.packet_index < 3 {
        return Err(invalid_input());
    }
    Ok(())
}

fn read_exact_or_eof(reader: &mut impl Read, mut target: &mut [u8]) -> io::Result<bool> {
    let mut read_any = false;
    while !target.is_empty() {
        match reader.read(target) {
            Ok(0) if !read_any => return Ok(false),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => {
                read_any = true;
                target = &mut target[count..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

fn validate_opus_header(header: &[u8]) -> LfResult<()> {
    if header.len() < 19 || &header[..8] != b"OpusHead" {
        return Err(LfError::AudioFormatUnsupported(
            "The Ogg stream does not contain Opus audio.".into(),
        ));
    }
    if header[8] != 1 || header[18] != 0 {
        return Err(unsupported_profile());
    }
    match header[9] {
        1 | 2 => Ok(()),
        _ => Err(LfError::AudioChannelsUnsupported(
            "Этот вариант аудиофайла пока не поддерживается".into(),
        )),
    }
}

fn validate_opus_tags(header: &[u8]) -> LfResult<()> {
    if header.len() < 16 || &header[..8] != b"OpusTags" {
        return Err(invalid_input());
    }
    let vendor_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    let comments_offset = 12_usize.checked_add(vendor_len).ok_or_else(invalid_input)?;
    let comments_end = comments_offset.checked_add(4).ok_or_else(invalid_input)?;
    if comments_end > header.len() {
        return Err(invalid_input());
    }
    Ok(())
}

fn ogg_crc(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

const FIR_TAPS: usize = 127;
const FIR_HALF: usize = FIR_TAPS / 2;

#[derive(Debug)]
struct FirResampler48To16 {
    samples: VecDeque<f32>,
    base_index: usize,
    total_input: usize,
    next_center: usize,
    produced: usize,
    taps: [f32; FIR_TAPS],
}

impl FirResampler48To16 {
    fn new() -> Self {
        let mut taps = [0.0; FIR_TAPS];
        let cutoff = 0.15_f64;
        let mut sum = 0.0_f64;
        for (index, tap) in taps.iter_mut().enumerate() {
            let x = index as f64 - FIR_HALF as f64;
            let sinc = if x == 0.0 {
                2.0 * cutoff
            } else {
                (2.0 * std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x)
            };
            let phase = 2.0 * std::f64::consts::PI * index as f64 / (FIR_TAPS - 1) as f64;
            let window = 0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos();
            *tap = (sinc * window) as f32;
            sum += sinc * window;
        }
        for tap in &mut taps {
            *tap /= sum as f32;
        }
        Self {
            samples: VecDeque::new(),
            base_index: 0,
            total_input: 0,
            next_center: 0,
            produced: 0,
            taps,
        }
    }

    fn push(
        &mut self,
        input: &[f32],
        output: &mut Vec<f32>,
        options: &DecodeOptions,
    ) -> LfResult<()> {
        options.checkpoint()?;
        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(invalid_output());
        }
        self.total_input = self.total_input.checked_add(input.len()).ok_or_else(|| {
            LfError::AudioLimitExceeded("Decoded audio sample count overflowed.".into())
        })?;
        self.samples.extend(input.iter().copied());
        while self.next_center.saturating_add(FIR_HALF) < self.total_input {
            check_output_budget(output.len(), 1, options)?;
            output.try_reserve(1).map_err(|_| {
                LfError::AudioLimitExceeded("Unable to allocate decoded audio output.".into())
            })?;
            output.push(self.sample_at_center(self.next_center));
            self.produced += 1;
            self.next_center = self.next_center.checked_add(3).ok_or_else(|| {
                LfError::AudioLimitExceeded("Resampler position overflowed.".into())
            })?;
            self.discard_consumed();
            if self.produced % 1024 == 0 {
                options.checkpoint()?;
            }
        }
        Ok(())
    }

    fn finish(&mut self, output: &mut Vec<f32>, options: &DecodeOptions) -> LfResult<()> {
        let expected = self.total_input.saturating_add(2) / 3;
        while self.produced < expected {
            options.checkpoint()?;
            check_output_budget(output.len(), 1, options)?;
            output.try_reserve(1).map_err(|_| {
                LfError::AudioLimitExceeded("Unable to allocate decoded audio output.".into())
            })?;
            output.push(self.sample_at_center(self.next_center));
            self.produced += 1;
            self.next_center += 3;
            self.discard_consumed();
        }
        Ok(())
    }

    fn sample_at_center(&self, center: usize) -> f32 {
        let mut value = 0.0_f32;
        for (tap_index, tap) in self.taps.iter().enumerate() {
            let relative = tap_index as isize - FIR_HALF as isize;
            let source = center as isize + relative;
            if source < 0 || source >= self.total_input as isize {
                continue;
            }
            let source = source as usize;
            if let Some(sample) = source
                .checked_sub(self.base_index)
                .and_then(|index| self.samples.get(index))
            {
                value += *sample * *tap;
            }
        }
        value
    }

    fn discard_consumed(&mut self) {
        let keep_from = self.next_center.saturating_sub(FIR_HALF);
        while self.base_index < keep_from && !self.samples.is_empty() {
            self.samples.pop_front();
            self.base_index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn options(max_output_samples: usize) -> DecodeOptions {
        DecodeOptions {
            max_output_samples,
            ..DecodeOptions::default()
        }
    }

    #[test]
    fn resampler_preserves_packet_state_and_length() {
        let input: Vec<f32> = (0..48_001).map(|i| (i as f32 * 0.01).sin()).collect();
        let mut one = FirResampler48To16::new();
        let mut expected = Vec::new();
        one.push(&input, &mut expected, &options(20_000)).unwrap();
        one.finish(&mut expected, &options(20_000)).unwrap();

        let mut chunked = FirResampler48To16::new();
        let mut actual = Vec::new();
        for chunk in input.chunks(317) {
            chunked.push(chunk, &mut actual, &options(20_000)).unwrap();
        }
        chunked.finish(&mut actual, &options(20_000)).unwrap();
        assert_eq!(actual.len(), 16_001);
        assert_eq!(actual, expected);
    }

    #[test]
    fn resampler_suppresses_ten_kilohertz() {
        let frames = 48_000;
        let input: Vec<f32> = (0..frames)
            .map(|i| (2.0 * std::f32::consts::PI * 10_000.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut resampler = FirResampler48To16::new();
        let mut output = Vec::new();
        resampler
            .push(&input, &mut output, &options(20_000))
            .unwrap();
        resampler.finish(&mut output, &options(20_000)).unwrap();
        let settled = &output[100..output.len() - 100];
        let rms = (settled.iter().map(|x| x * x).sum::<f32>() / settled.len() as f32).sqrt();
        let attenuation_db = 20.0 * (rms / (0.5_f32).sqrt()).log10();
        assert!(
            attenuation_db <= -40.0,
            "attenuation was {attenuation_db:.2} dB"
        );
    }

    #[test]
    fn cancellation_is_distinct() {
        let options = DecodeOptions::default();
        options.cancellation.store(true, Ordering::Relaxed);
        assert_eq!(options.checkpoint().unwrap_err().code(), "AUDIO_CANCELLED");
    }

    #[test]
    fn expired_deadline_is_distinct() {
        let options = DecodeOptions {
            deadline: Some(Instant::now()),
            ..DecodeOptions::default()
        };
        assert_eq!(
            options.checkpoint().unwrap_err().code(),
            "AUDIO_DECODE_TIMEOUT"
        );
    }

    #[test]
    fn rejects_random_bytes_before_probe() {
        let error = decode_bytes(b"not ogg", &DecodeOptions::default()).unwrap_err();
        assert_eq!(error.code(), "AUDIO_INPUT_INVALID");
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/ogg_opus")
            .join(name)
    }

    fn decode_fixture_48k(name: &str) -> Vec<f32> {
        let bytes = std::fs::read(fixture(name)).unwrap();
        let options = options(20_000);
        validate_ogg(Cursor::new(&bytes), &options).unwrap();
        let mut pcm = Vec::new();
        decode_source(Box::new(Cursor::new(&bytes)), &options, Some(&mut pcm)).unwrap();
        pcm
    }

    fn reference_f32le(name: &str) -> Vec<f32> {
        std::fs::read(fixture(name))
            .unwrap()
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect()
    }

    #[test]
    fn fixture_manifest_hashes_every_binary_artifact() {
        let directory = fixture("manifest.json").parent().unwrap().to_path_buf();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap())
                .unwrap();
        let fixtures = manifest["fixtures"].as_array().unwrap();
        let listed = fixtures
            .iter()
            .map(|entry| entry["file"].as_str().unwrap())
            .collect::<std::collections::HashSet<_>>();

        for entry in fixtures {
            let name = entry["file"].as_str().unwrap();
            let bytes = std::fs::read(directory.join(name)).unwrap();
            let digest = format!("{:x}", Sha256::digest(bytes));
            assert_eq!(digest, entry["sha256"].as_str().unwrap(), "hash for {name}");
        }
        for entry in std::fs::read_dir(directory).unwrap() {
            let name = entry.unwrap().file_name();
            let name = name.to_str().unwrap();
            if name != "manifest.json" {
                assert!(listed.contains(name), "fixture {name} is missing from manifest");
            }
        }
    }

    #[test]
    fn decodes_mono_fixture_with_exact_timeline() {
        let pcm = decode_path(&fixture("mono-997hz-1s.opus"), &options(20_000)).unwrap();
        assert_eq!(pcm.len(), 16_000);
        assert!(pcm.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn decodes_stereo_fixture_to_mono() {
        let pcm = decode_path(&fixture("stereo-440-880hz-1s.ogg"), &options(20_000)).unwrap();
        assert_eq!(pcm.len(), 16_000);
        assert!(pcm.iter().any(|sample| sample.abs() > 0.01));
    }

    #[test]
    fn decoded_48k_matches_pinned_libopus_reference() {
        for (input, reference) in [
            ("mono-997hz-1s.opus", "mono-997hz-1s.reference.f32le"),
            (
                "stereo-440-880hz-1s.ogg",
                "stereo-440-880hz-1s.reference.f32le",
            ),
            (
                "mono-997hz-1s-gain-plus-6db.opus",
                "mono-997hz-1s-gain-plus-6db.reference.f32le",
            ),
            (
                "mono-997hz-1s-gain-minus-6db.opus",
                "mono-997hz-1s-gain-minus-6db.reference.f32le",
            ),
            (
                "mono-chirp-vbr-60ms-1.2s.opus",
                "mono-chirp-vbr-60ms-1.2s.reference.f32le",
            ),
        ] {
            let actual = decode_fixture_48k(input);
            let expected = reference_f32le(reference);
            assert_eq!(
                actual.len(),
                expected.len(),
                "timeline mismatch for {input}"
            );
            let max_difference = actual
                .iter()
                .zip(&expected)
                .map(|(left, right)| (left - right).abs())
                .fold(0.0_f32, f32::max);
            assert!(
                max_difference <= 1e-5,
                "{input} max absolute difference was {max_difference}"
            );
        }
    }

    #[test]
    fn decodes_short_silence_without_empty_output() {
        let pcm = decode_path(&fixture("mono-silence-80ms.opus"), &options(2_000)).unwrap();
        assert_eq!(pcm.len(), 1_280);
        assert!(pcm.iter().all(|sample| sample.abs() < 1e-5));
    }

    #[test]
    fn decodes_vbr_sixty_millisecond_packets() {
        let pcm = decode_path(
            &fixture("mono-chirp-vbr-60ms-1.2s.opus"),
            &options(20_000),
        )
        .unwrap();
        assert_eq!(pcm.len(), 19_200);
        assert!(pcm.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn detects_crc_corruption() {
        let mut bytes = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        let error = decode_bytes(&bytes, &options(20_000)).unwrap_err();
        assert_eq!(error.code(), "AUDIO_INPUT_INVALID");
    }

    #[test]
    fn enforces_output_budget_before_success() {
        let error = decode_path(&fixture("mono-997hz-1s.opus"), &options(100)).unwrap_err();
        assert_eq!(error.code(), "AUDIO_LIMIT_EXCEEDED");
    }

    #[test]
    fn unicode_path_and_content_probe_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("голос с пробелом.bin");
        std::fs::copy(fixture("mono-997hz-1s.opus"), &path).unwrap();
        let pcm = decode_path(&path, &options(20_000)).unwrap();
        assert_eq!(pcm.len(), 16_000);
    }

    fn patch_identification_header(bytes: &mut [u8], edit: impl FnOnce(&mut [u8])) {
        assert_eq!(&bytes[..4], b"OggS");
        let segments = usize::from(bytes[26]);
        let body_start = 27 + segments;
        let body_len: usize = bytes[27..body_start]
            .iter()
            .map(|value| usize::from(*value))
            .sum();
        let page_end = body_start + body_len;
        edit(&mut bytes[body_start..page_end]);
        bytes[22..26].fill(0);
        let crc = ogg_crc(0, &bytes[..page_end]);
        bytes[22..26].copy_from_slice(&crc.to_le_bytes());
    }

    fn page_ranges(bytes: &[u8]) -> Vec<std::ops::Range<usize>> {
        let mut ranges = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            assert_eq!(&bytes[offset..offset + 4], b"OggS");
            let segments = usize::from(bytes[offset + 26]);
            let header_end = offset + 27 + segments;
            let body_len = bytes[offset + 27..header_end]
                .iter()
                .map(|value| usize::from(*value))
                .sum::<usize>();
            let end = header_end + body_len;
            ranges.push(offset..end);
            offset = end;
        }
        ranges
    }

    fn patch_page(bytes: &mut [u8], page_index: usize, edit: impl FnOnce(&mut [u8])) {
        let range = page_ranges(bytes)[page_index].clone();
        let page = &mut bytes[range];
        edit(page);
        page[22..26].fill(0);
        let crc = ogg_crc(0, page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
    }

    fn make_page(
        serial: u32,
        sequence: u32,
        flags: u8,
        granule: u64,
        lacing: &[u8],
        body: &[u8],
    ) -> Vec<u8> {
        assert_eq!(
            lacing
                .iter()
                .map(|value| usize::from(*value))
                .sum::<usize>(),
            body.len()
        );
        let mut page = Vec::with_capacity(27 + lacing.len() + body.len());
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(flags);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&serial.to_le_bytes());
        page.extend_from_slice(&sequence.to_le_bytes());
        page.extend_from_slice(&0_u32.to_le_bytes());
        page.push(lacing.len().try_into().unwrap());
        page.extend_from_slice(lacing);
        page.extend_from_slice(body);
        let crc = ogg_crc(0, &page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        page
    }

    #[test]
    fn applies_positive_and_negative_header_output_gain_once() {
        let original = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        let mut louder_bytes = original.clone();
        patch_identification_header(&mut louder_bytes, |header| {
            header[16..18].copy_from_slice(&1536_i16.to_le_bytes());
        });
        let mut quieter_bytes = original.clone();
        patch_identification_header(&mut quieter_bytes, |header| {
            header[16..18].copy_from_slice(&(-1536_i16).to_le_bytes());
        });
        let plain = decode_bytes(&original, &options(20_000)).unwrap();
        let louder = decode_bytes(&louder_bytes, &options(20_000)).unwrap();
        let quieter = decode_bytes(&quieter_bytes, &options(20_000)).unwrap();
        let rms = |samples: &[f32]| {
            (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32)
                .sqrt()
        };
        let louder_ratio = rms(&louder[100..15_900]) / rms(&plain[100..15_900]);
        let quieter_ratio = rms(&quieter[100..15_900]) / rms(&plain[100..15_900]);
        let expected_louder = 10.0_f32.powf(6.0 / 20.0);
        let expected_quieter = 10.0_f32.powf(-6.0 / 20.0);
        assert!((louder_ratio - expected_louder).abs() < 1e-4);
        assert!((quieter_ratio - expected_quieter).abs() < 1e-4);
    }

    #[test]
    fn informational_input_rate_does_not_change_the_timeline() {
        let original = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        let expected = decode_bytes(&original, &options(20_000)).unwrap();
        assert_eq!(
            decode_path(
                &fixture("mono-997hz-1s-input-rate-44100.opus"),
                &options(20_000)
            )
            .unwrap(),
            expected
        );
        for rate in [0_u32, 8_000, 44_100, 192_000, 10_000_000] {
            let mut changed = original.clone();
            patch_identification_header(&mut changed, |header| {
                header[12..16].copy_from_slice(&rate.to_le_bytes());
            });
            assert_eq!(decode_bytes(&changed, &options(20_000)).unwrap(), expected);
        }
    }

    #[test]
    fn valid_nonzero_initial_granule_keeps_audio_samples() {
        let original = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        let expected = decode_bytes(&original, &options(20_000)).unwrap();
        assert_eq!(
            decode_path(
                &fixture("mono-997hz-1s-initial-granule.opus"),
                &options(20_000)
            )
            .unwrap(),
            expected
        );
        let mut shifted = original.clone();
        for page_index in 2..page_ranges(&shifted).len() {
            patch_page(&mut shifted, page_index, |page| {
                let granule = u64::from_le_bytes(page[6..14].try_into().unwrap());
                page[6..14].copy_from_slice(&(granule + 48_000).to_le_bytes());
            });
        }
        assert_eq!(decode_bytes(&shifted, &options(20_000)).unwrap(), expected);
    }

    #[test]
    fn rejects_decreasing_audio_granule() {
        let mut bytes = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        patch_page(&mut bytes, 3, |page| {
            page[6..14].copy_from_slice(&47_999_u64.to_le_bytes());
        });
        assert_eq!(
            decode_bytes(&bytes, &options(20_000)).unwrap_err().code(),
            "AUDIO_INPUT_INVALID"
        );
    }

    #[test]
    fn rejects_inconsistent_continued_packet_flag() {
        let mut bytes = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        patch_page(&mut bytes, 3, |page| page[5] |= 0x01);
        assert_eq!(
            decode_bytes(&bytes, &options(20_000)).unwrap_err().code(),
            "AUDIO_INPUT_INVALID"
        );
    }

    #[test]
    fn rejects_oversized_comment_packet_before_decode() {
        let source = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        let first_page = page_ranges(&source)[0].clone();
        let mut bytes = source[first_page].to_vec();
        let serial = u32::from_le_bytes(bytes[14..18].try_into().unwrap());
        for page_index in 0..17_u32 {
            let mut body = vec![0_u8; 255 * 255];
            if page_index == 0 {
                body[..8].copy_from_slice(b"OpusTags");
            }
            bytes.extend(make_page(
                serial,
                page_index + 1,
                if page_index == 0 { 0 } else { 0x01 },
                u64::MAX,
                &[255; 255],
                &body,
            ));
        }
        assert_eq!(
            decode_bytes(&bytes, &DecodeOptions::default())
                .unwrap_err()
                .code(),
            "AUDIO_LIMIT_EXCEEDED"
        );
    }

    #[test]
    fn carries_pre_skip_across_packet_boundary() {
        let mut bytes = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        patch_identification_header(&mut bytes, |header| {
            header[10..12].copy_from_slice(&1_600_u16.to_le_bytes());
        });
        let pcm = decode_bytes(&bytes, &options(20_000)).unwrap();
        assert_eq!(pcm.len(), 15_571);
    }

    #[test]
    fn rejects_nonzero_mapping_family_before_decode() {
        let mut bytes = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        patch_identification_header(&mut bytes, |header| header[18] = 1);
        let error = decode_bytes(&bytes, &options(20_000)).unwrap_err();
        assert_eq!(error.code(), "AUDIO_FORMAT_UNSUPPORTED");
    }

    #[test]
    fn rejects_chained_stream_instead_of_returning_first_part() {
        let mut bytes = std::fs::read(fixture("mono-silence-80ms.opus")).unwrap();
        bytes.extend(std::fs::read(fixture("mono-silence-80ms.opus")).unwrap());
        let error = decode_bytes(&bytes, &options(4_000)).unwrap_err();
        assert_eq!(error.code(), "AUDIO_FORMAT_UNSUPPORTED");
    }

    #[test]
    fn rejects_truncated_page() {
        let mut bytes = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        bytes.truncate(bytes.len() - 10);
        let error = decode_bytes(&bytes, &options(20_000)).unwrap_err();
        assert_eq!(error.code(), "AUDIO_INPUT_INVALID");
    }

    #[test]
    fn cancelled_job_does_not_cancel_independent_job() {
        let cancelled = DecodeOptions::default();
        cancelled.cancellation.store(true, Ordering::Relaxed);
        let active = options(20_000);
        assert_eq!(
            decode_path(&fixture("mono-997hz-1s.opus"), &cancelled)
                .unwrap_err()
                .code(),
            "AUDIO_CANCELLED"
        );
        assert_eq!(
            decode_path(&fixture("mono-997hz-1s.opus"), &active)
                .unwrap()
                .len(),
            16_000
        );
    }

    #[test]
    fn malformed_header_smoke_never_panics() {
        let source = std::fs::read(fixture("mono-997hz-1s.opus")).unwrap();
        for index in 0..source.len().min(512) {
            let mut mutated = source.clone();
            mutated[index] ^= 0x5a;
            let _ = decode_bytes(&mutated, &options(20_000));
        }
        for len in 0..128 {
            let bytes: Vec<u8> = (0..len).map(|index| (index * 73 + 19) as u8).collect();
            let _ = decode_bytes(&bytes, &options(20_000));
        }
    }
}
