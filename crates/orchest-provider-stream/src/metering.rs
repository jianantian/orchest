//! Voice usage tracked by the driver that owns the provider connection.

use orchest_protocol::{
    AudioFormat, EventStream, VoiceOutcome, VoiceQuantity, VoiceUsage, VoiceUsageSource,
};
use orchest_provider_core::voice::{pcm_duration_ms, VoicePrice};
use tokio::sync::oneshot;

#[derive(Default)]
pub struct VoiceMeter {
    settlement: Option<Settlement>,
    audio_bytes: u64,
    cancelled: bool,
    submitted_characters: Option<u64>,
}

struct Settlement {
    sender: oneshot::Sender<VoiceUsage>,
    usage: VoiceUsage,
    pcm: Option<(u64, u64)>,
    price: Option<VoicePrice>,
}

impl VoiceMeter {
    pub(crate) fn attach(
        events: EventStream,
        provider: &str,
        model: &str,
        pcm: Option<(u64, u64)>,
        price: Option<VoicePrice>,
    ) -> (Self, EventStream) {
        let (sender, receiver) = oneshot::channel();
        let meter = Self {
            settlement: Some(Settlement {
                sender,
                usage: VoiceUsage::new(provider, model, VoiceOutcome::Failed),
                pcm,
                price,
            }),
            ..Self::default()
        };
        (meter, events.with_voice_usage(receiver))
    }

    pub(crate) fn model(&self) -> Option<&str> {
        self.settlement
            .as_ref()
            .map(|settlement| settlement.usage.model.as_str())
    }

    pub(crate) fn audio_sent(&mut self, len: usize) {
        self.audio_bytes = self.audio_bytes.saturating_add(len as u64);
    }

    pub(crate) fn with_text(mut self, text: Option<&str>) -> Self {
        self.submitted_characters = text.map(|text| text.chars().count() as u64);
        self
    }

    pub(crate) fn text_sent(&mut self) {
        if let Some(characters) = self.submitted_characters.take() {
            self.add_characters(characters);
        }
    }

    pub(crate) fn add_text(&mut self, text: &str) {
        self.add_characters(text.chars().count() as u64);
    }

    fn add_characters(&mut self, characters: u64) {
        if let Some(settlement) = &mut self.settlement {
            let count = settlement.usage.input_characters.get_or_insert(0);
            *count = count.saturating_add(characters);
        }
    }

    pub(crate) fn reported_audio(&mut self, duration: Option<VoiceQuantity>) {
        if let (Some(settlement), Some(duration)) = (&mut self.settlement, duration) {
            settlement.usage.audio_duration_ms = Some(duration);
        }
    }

    pub(crate) fn billed_characters(&mut self, characters: Option<u64>) {
        if let (Some(settlement), Some(characters)) = (&mut self.settlement, characters) {
            settlement.usage.billed_characters = Some(characters);
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub(crate) fn complete(&mut self) {
        if let Some(settlement) = &mut self.settlement {
            settlement.usage.outcome = VoiceOutcome::Completed;
        }
    }
}

impl Drop for VoiceMeter {
    fn drop(&mut self) {
        let Some(Settlement {
            sender,
            mut usage,
            pcm,
            price,
        }) = self.settlement.take()
        else {
            return;
        };
        if self.cancelled {
            usage.outcome = VoiceOutcome::Cancelled;
        }
        if usage.audio_duration_ms.is_none() {
            usage.audio_duration_ms = pcm
                .filter(|_| self.audio_bytes > 0 || usage.outcome == VoiceOutcome::Completed)
                .and_then(|(sample_rate, size)| {
                    pcm_duration_ms(self.audio_bytes, sample_rate, size)
                })
                .map(|value| VoiceQuantity {
                    value,
                    source: VoiceUsageSource::InputEstimate,
                });
        }
        usage.cost_estimate = price.and_then(|price| price.estimate(&usage));
        let _ = sender.send(usage);
    }
}

pub(crate) fn pcm16(format: AudioFormat, sample_rate: u64, channels: u64) -> Option<(u64, u64)> {
    matches!(format, AudioFormat::Pcm | AudioFormat::Pcm16Le)
        .then_some((sample_rate, channels.checked_mul(2)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn text_increments_count_unicode_scalars_and_keep_billing_separate() {
        let (_, events) = EventStream::channel(2);
        let (mut meter, mut events) = VoiceMeter::attach(events, "fake", "m", None, None);
        meter.add_text("你 ");
        meter.add_text("");
        meter.add_text("e\u{301}🙂");
        meter.billed_characters(Some(7));
        meter.billed_characters(None);
        meter.complete();
        drop(meter);

        let usage = events.take_voice_usage().await.unwrap();
        assert_eq!(usage.input_characters, Some(5));
        assert_eq!(usage.billed_characters, Some(7));
        assert!(usage.cost_estimate.is_none());
    }

    #[tokio::test]
    async fn a_cancelled_attempt_keeps_its_own_audio_total() {
        let (_, events) = EventStream::channel(2);
        let (mut first, mut events) =
            VoiceMeter::attach(events, "fake", "m", Some((16_000, 2)), None);
        first.audio_sent(3_200);
        first.audio_sent(0);
        first.cancel();
        drop(first);
        let usage = events.take_voice_usage().await.unwrap();
        assert_eq!(usage.outcome, VoiceOutcome::Cancelled);
        assert_eq!(
            usage.audio_duration_ms,
            Some(VoiceQuantity {
                value: 100,
                source: VoiceUsageSource::InputEstimate
            }),
        );
        assert!(events.take_voice_usage().await.is_none());

        let (_, events) = EventStream::channel(2);
        let (mut retry, mut events) =
            VoiceMeter::attach(events, "fake", "m", Some((16_000, 2)), None);
        retry.audio_sent(0);
        retry.audio_sent(6_400);
        retry.complete();
        drop(retry);
        let usage = events.take_voice_usage().await.unwrap();
        assert_eq!(usage.outcome, VoiceOutcome::Completed);
        assert_eq!(usage.audio_duration_ms.unwrap().value, 200);
    }
}
