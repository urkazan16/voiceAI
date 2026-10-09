// Derived from symphonia-adapter-libopus 0.3.0 under Apache-2.0.
#![warn(missing_debug_implementations)]

use std::fmt;

use symphonia_core::audio::{
    layouts, AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec, Channels,
    GenericAudioBufferRef,
};
use symphonia_core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia_core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult,
};
use symphonia_core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia_core::codecs::CodecInfo;
use symphonia_core::errors::{decode_error, unsupported_error, Result};
use symphonia_core::{support_audio_codec, packet::PacketRef};

use crate::decoder::Decoder;

mod decoder;

const SAMPLE_RATE: usize = 48_000;
const DEFAULT_SAMPLES_PER_CHANNEL: usize = SAMPLE_RATE * 20 / 1000;
const MAX_SAMPLES_PER_CHANNEL: usize = SAMPLE_RATE * 120 / 1000;
const MAX_CHANNELS: usize = 2;
const PCM_CAPACITY: usize = MAX_SAMPLES_PER_CHANNEL * MAX_CHANNELS;

#[derive(Debug, Clone, Copy, PartialEq)]
struct OpusHeader {
    channels: usize,
    pre_skip: usize,
    gain: f32,
}

fn parse_header(buf: &[u8]) -> Result<OpusHeader> {
    if buf.len() < 19 || &buf[..8] != b"OpusHead" {
        return decode_error("opus: invalid identification header");
    }
    if buf[8] != 1 {
        return unsupported_error("opus: unsupported identification header version");
    }
    let channels = usize::from(buf[9]);
    if !(1..=MAX_CHANNELS).contains(&channels) {
        return unsupported_error("opus: unsupported number of channels");
    }
    if buf[18] != 0 {
        return unsupported_error("opus: unsupported channel mapping family");
    }
    let pre_skip = usize::from(u16::from_le_bytes([buf[10], buf[11]]));
    let gain_q8 = i16::from_le_bytes([buf[16], buf[17]]) as f32;
    let gain = 10.0_f32.powf(gain_q8 / (20.0 * 256.0));
    if !gain.is_finite() {
        return decode_error("opus: invalid output gain");
    }
    Ok(OpusHeader { channels, pre_skip, gain })
}

/// Symphonia-compatible wrapper for libopus, pinned and patched by LocalFlow.
pub struct OpusDecoder {
    params: AudioCodecParameters,
    decoder: Decoder,
    buf: AudioBuffer<f32>,
    pcm: [f32; PCM_CAPACITY],
    samples_per_channel: usize,
    sample_rate: u32,
    num_channels: usize,
    initial_pre_skip: usize,
    remaining_pre_skip: usize,
    gain: f32,
}

impl fmt::Debug for OpusDecoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpusDecoder")
            .field("params", &self.params)
            .field("decoder", &self.decoder)
            .field("samples_per_channel", &self.samples_per_channel)
            .field("sample_rate", &self.sample_rate)
            .field("num_channels", &self.num_channels)
            .field("remaining_pre_skip", &self.remaining_pre_skip)
            .field("gain", &self.gain)
            .finish_non_exhaustive()
    }
}

impl OpusDecoder {
    fn try_new(params: &AudioCodecParameters, _opts: &AudioDecoderOptions) -> Result<Self> {
        let extra_data = params
            .extra_data
            .as_deref()
            .ok_or(symphonia_core::errors::Error::DecodeError(
                "opus: identification header required",
            ))?;
        let header = parse_header(extra_data)?;
        let param_channels = params
            .channels
            .as_ref()
            .map(Channels::count)
            .ok_or(symphonia_core::errors::Error::Unsupported(
                "opus: channels required",
            ))?;
        if param_channels != header.channels {
            return decode_error("opus: channel count mismatch");
        }
        let sample_rate = params.sample_rate.ok_or(
            symphonia_core::errors::Error::Unsupported("opus: sample rate required"),
        )?;
        if sample_rate != SAMPLE_RATE as u32 {
            return unsupported_error("opus: only 48000 Hz decoding is supported");
        }
        Ok(Self {
            params: params.to_owned(),
            decoder: Decoder::new(sample_rate, header.channels as u32)?,
            buf: audio_buffer(sample_rate, DEFAULT_SAMPLES_PER_CHANNEL, header.channels),
            pcm: [0.0; PCM_CAPACITY],
            samples_per_channel: DEFAULT_SAMPLES_PER_CHANNEL,
            sample_rate,
            num_channels: header.channels,
            initial_pre_skip: header.pre_skip,
            remaining_pre_skip: header.pre_skip,
            gain: header.gain,
        })
    }
}

impl AudioDecoder for OpusDecoder {
    fn codec_info(&self) -> &CodecInfo {
        &Self::supported_codecs()[0].info
    }

    fn reset(&mut self) {
        self.decoder.reset();
        self.remaining_pre_skip = self.initial_pre_skip;
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        let samples_per_channel = self.decoder.decode(packet.data, &mut self.pcm)?;
        if samples_per_channel != self.samples_per_channel {
            self.buf = audio_buffer(self.sample_rate, samples_per_channel, self.num_channels);
            self.samples_per_channel = samples_per_channel;
        }

        let samples = samples_per_channel * self.num_channels;
        if self.gain != 1.0 {
            for sample in &mut self.pcm[..samples] {
                *sample *= self.gain;
            }
        }

        self.buf.clear();
        self.buf.render_uninit(None);
        let pcm = &self.pcm[..samples];
        self.buf.copy_from_slice_interleaved(&pcm);

        let packet_trim = (packet.trim_start.get() as usize).min(samples_per_channel);
        let available_after_packet_trim = samples_per_channel - packet_trim;
        let pre_skip_now = self.remaining_pre_skip.min(available_after_packet_trim);
        self.remaining_pre_skip -= pre_skip_now;
        self.buf.trim(
            packet_trim + pre_skip_now,
            packet.trim_end.get() as usize,
        );
        Ok(self.buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

impl RegisterableAudioDecoder for OpusDecoder {
    fn try_registry_new(
        params: &AudioCodecParameters,
        opts: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>> {
        Ok(Box::new(Self::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        &[support_audio_codec!(CODEC_ID_OPUS, "opus", "Opus")]
    }
}

fn audio_buffer(
    sample_rate: u32,
    samples_per_channel: usize,
    num_channels: usize,
) -> AudioBuffer<f32> {
    let channels = match num_channels {
        1 => layouts::CHANNEL_LAYOUT_MONO,
        2 => layouts::CHANNEL_LAYOUT_STEREO,
        _ => unreachable!("validated channel count"),
    };
    AudioBuffer::new(AudioSpec::new(sample_rate, channels), samples_per_channel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(pre_skip: u16, gain: i16, mapping: u8) -> [u8; 19] {
        let mut bytes = [0; 19];
        bytes[..8].copy_from_slice(b"OpusHead");
        bytes[8] = 1;
        bytes[9] = 2;
        bytes[10..12].copy_from_slice(&pre_skip.to_le_bytes());
        bytes[12..16].copy_from_slice(&48_000_u32.to_le_bytes());
        bytes[16..18].copy_from_slice(&gain.to_le_bytes());
        bytes[18] = mapping;
        bytes
    }

    #[test]
    fn parses_gain_and_pre_skip() {
        let parsed = parse_header(&header(7_000, 256, 0)).unwrap();
        assert_eq!(parsed.pre_skip, 7_000);
        assert!((parsed.gain - 10.0_f32.powf(1.0 / 20.0)).abs() < 1e-6);
    }

    #[test]
    fn rejects_nonzero_mapping_family() {
        assert!(parse_header(&header(312, 0, 1)).is_err());
    }
}
