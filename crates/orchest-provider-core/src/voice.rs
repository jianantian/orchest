//! Shared metering and price application for voice adapters.

use orchest_protocol::{
    AudioFormat, VoiceCostEstimate, VoiceOutcome, VoiceQuantity, VoiceRate, VoiceRateUnit,
    VoiceUsage, VoiceUsageSource,
};

use crate::pricing::{Meter, Pricing};

/// A cataloged public list rate. Endpoint and model applicability are checked
/// by the adapter before it applies a rate to live usage.
#[derive(Debug, Clone, Copy)]
pub struct VoicePrice {
    pub currency: &'static str,
    pub unit: VoiceRateUnit,
    pub price_per_unit: f64,
    pub scope: &'static str,
    pub source_url: &'static str,
}

impl VoicePrice {
    pub fn rate(self) -> VoiceRate {
        VoiceRate {
            currency: self.currency.into(),
            unit: self.unit,
            price_per_unit: self.price_per_unit,
            scope: self.scope.into(),
            source_url: self.source_url.into(),
        }
    }

    pub fn estimate(self, usage: &VoiceUsage) -> Option<VoiceCostEstimate> {
        if usage.outcome != VoiceOutcome::Completed {
            return None;
        }
        let currency = self.currency.into();
        let cost = match self.unit {
            VoiceRateUnit::AudioSecond => Pricing::Duration {
                currency,
                per_second: self.price_per_unit,
            }
            .cost(Meter::DurationMs(usage.audio_duration_ms?.value)),
            VoiceRateUnit::BilledCharacter => Pricing::Characters {
                currency,
                per_character: self.price_per_unit,
            }
            .cost(Meter::Characters(usage.billed_characters?)),
            _ => return None,
        };
        Some(VoiceCostEstimate {
            amount: cost.amount,
            rate: self.rate(),
        })
    }
}

pub fn pcm_duration_ms(bytes: u64, sample_rate: u64, bytes_per_frame: u64) -> Option<u64> {
    if sample_rate == 0 || bytes.checked_rem(bytes_per_frame) != Some(0) {
        return None;
    }
    let frames = u128::from(bytes / bytes_per_frame);
    u64::try_from(frames * 1000 / u128::from(sample_rate)).ok()
}

/// Recognize complete RIFF/WAVE with a PCM fmt chunk and a data chunk.
pub fn wav_duration_ms(bytes: &[u8]) -> Option<u64> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let len = usize::try_from(read_u32(bytes.get(4..8)?)?)
        .ok()?
        .checked_add(8)?;
    let mut chunks = bytes.get(12..len)?;
    let mut format = None;
    let mut data_bytes = None;
    while !chunks.is_empty() {
        let size = usize::try_from(read_u32(chunks.get(4..8)?)?).ok()?;
        let end = 8usize.checked_add(size)?;
        let data = chunks.get(8..end)?;
        match chunks.get(..4)? {
            b"fmt " => {
                if read_u16(data.get(..2)?)? != 1 {
                    return None;
                }
                let channels = u64::from(read_u16(data.get(2..4)?)?);
                let sample_rate = u64::from(read_u32(data.get(4..8)?)?);
                let block_align = u64::from(read_u16(data.get(12..14)?)?);
                let bits = u64::from(read_u16(data.get(14..16)?)?);
                if channels == 0 || bits == 0 || block_align != channels * bits.div_ceil(8) {
                    return None;
                }
                format = Some((sample_rate, block_align));
            }
            b"data" => data_bytes = Some(size as u64),
            _ => {}
        }
        chunks = chunks.get(end.checked_add(size % 2)?..)?;
    }
    let (sample_rate, block_align) = format?;
    pcm_duration_ms(data_bytes?, sample_rate, block_align)
}

fn read_u16(bytes: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

pub fn batch_audio_usage(
    provider: &str,
    model: &str,
    format: AudioFormat,
    audio: &[u8],
    reported: Option<VoiceQuantity>,
) -> VoiceUsage {
    let mut usage = VoiceUsage::new(provider, model, VoiceOutcome::Completed);
    usage.audio_duration_ms = reported.or_else(|| {
        if !matches!(format, AudioFormat::Wav | AudioFormat::WavPcm16Le) {
            return None;
        }
        Some(VoiceQuantity {
            value: wav_duration_ms(audio)?,
            source: VoiceUsageSource::InputEstimate,
        })
    });
    usage
}

/// Normalize an upstream duration in seconds to the closest millisecond.
pub fn duration_from_seconds(seconds: f64, source: VoiceUsageSource) -> Option<VoiceQuantity> {
    let ms = (seconds * 1000.0).round();
    (seconds >= 0.0 && ms.is_finite() && ms < u64::MAX as f64).then_some(VoiceQuantity {
        value: ms as u64,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav() -> Vec<u8> {
        let mut wav = b"RIFF".to_vec();
        wav.extend(32_036u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(16_000u32.to_le_bytes());
        wav.extend(32_000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(32_000u32.to_le_bytes());
        wav.resize(32_044, 0);
        wav
    }

    #[test]
    fn batch_duration_prefers_provider_over_complete_pcm_wav() {
        let wav = wav();
        let estimated = batch_audio_usage("fake", "m", AudioFormat::Wav, &wav, None);
        assert_eq!(
            estimated.audio_duration_ms,
            Some(VoiceQuantity {
                value: 1000,
                source: VoiceUsageSource::InputEstimate
            })
        );
        let reported = duration_from_seconds(1.5, VoiceUsageSource::ProviderReported);
        let result = batch_audio_usage("fake", "m", AudioFormat::Wav, &wav, reported);
        assert_eq!(result.audio_duration_ms, reported);
        assert_eq!(wav_duration_ms(&wav[..wav.len() - 1]), None);
        let opaque = batch_audio_usage("fake", "m", AudioFormat::Mp3, &wav, None);
        assert_eq!(opaque.audio_duration_ms, None);
    }

    #[test]
    fn unknown_rates_and_billing_units_do_not_masquerade_as_zero_cost() {
        let price = VoicePrice {
            currency: "USD",
            unit: VoiceRateUnit::BilledCharacter,
            price_per_unit: 0.01,
            scope: "test",
            source_url: "fixture",
        };
        let mut usage = VoiceUsage::new("fake", "m", VoiceOutcome::Completed);
        usage.input_characters = Some(3);
        assert!(price.estimate(&usage).is_none());
        usage.billed_characters = Some(4);
        assert_eq!(price.estimate(&usage).unwrap().amount, 0.04);
        usage.outcome = VoiceOutcome::Cancelled;
        assert!(price.estimate(&usage).is_none());
    }
}
