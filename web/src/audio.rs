//! Procedural sound effects via the Web Audio API.
//!
//! Design brief (per the "ASMR-quality or it gets muted" rule): everything here
//! is soft, short, and low-gain — filtered noise for card whisks, gently muted
//! ticks for chips — with smooth envelopes and no harsh transients. No music.
//! All sounds are synthesized, so nothing is downloaded and the bundle stays
//! tiny. A single mute flag gates the whole module.
//!
//! WASM is single-threaded, so the `AudioContext` lives in a `thread_local`
//! rather than in the (Send+Sync) game state. The context is created lazily on
//! first use — which is always after the "Deal me in" click, satisfying the
//! browser's require-a-user-gesture rule for audio.

use std::cell::{Cell, RefCell};

use web_sys::{
    AudioBuffer, AudioContext, BiquadFilterNode, BiquadFilterType, GainNode, OscillatorNode,
    OscillatorType,
};

thread_local! {
    static CTX: RefCell<Option<AudioContext>> = const { RefCell::new(None) };
    static MUTED: Cell<bool> = const { Cell::new(false) };
    /// Cached one-shot white-noise buffer, reused for every noise burst.
    static NOISE: RefCell<Option<AudioBuffer>> = const { RefCell::new(None) };
}

pub fn set_muted(m: bool) {
    MUTED.with(|c| c.set(m));
}

pub fn is_muted() -> bool {
    MUTED.with(|c| c.get())
}

/// Get (creating on first call) the shared AudioContext. Returns `None` if the
/// browser has no Web Audio support.
fn ctx() -> Option<AudioContext> {
    CTX.with(|c| {
        if c.borrow().is_none() {
            if let Ok(new_ctx) = AudioContext::new() {
                *c.borrow_mut() = Some(new_ctx);
            }
        }
        c.borrow().clone()
    })
}

fn now(ctx: &AudioContext) -> f64 {
    ctx.current_time()
}

/// A short burst of low-pass-filtered white noise — the basis of card and chip
/// texture. `cutoff` shapes brightness; `peak` is the (small) peak gain; `dur`
/// is seconds. The envelope ramps up in ~4 ms and decays smoothly so there's no
/// click.
// `AudioBufferSourceNode::stop_with_when` is deprecated in web-sys in favor of
// the base-class binding, but it's the working way to schedule a stop here.
#[allow(deprecated)]
fn noise_burst(ctx: &AudioContext, cutoff: f32, peak: f32, dur: f64) {
    let Some(buffer) = noise_buffer(ctx) else { return };
    let Ok(src) = ctx.create_buffer_source() else { return };
    src.set_buffer(Some(&buffer));

    let Ok(filter) = ctx.create_biquad_filter() else { return };
    filter.set_type(BiquadFilterType::Lowpass);
    filter.frequency().set_value(cutoff);

    let Ok(gain) = ctx.create_gain() else { return };
    let t = now(ctx);
    let g = gain.gain();
    g.set_value(0.0001);
    let _ = g.linear_ramp_to_value_at_time(peak, t + 0.004);
    let _ = g.exponential_ramp_to_value_at_time(0.0001, t + dur);

    let _ = src.connect_with_audio_node(&filter);
    let _ = filter.connect_with_audio_node(&gain);
    let _ = gain.connect_with_audio_node(&ctx.destination());
    let _ = src.start();
    let _ = src.stop_with_when(t + dur + 0.02);
    let _ = keep(&filter, &gain);
}

/// A soft sine/triangle blip for chips — muted and brief, more "tick" than
/// "beep".
fn blip(ctx: &AudioContext, freq: f32, peak: f32, dur: f64, wave: OscillatorType) {
    let Ok(osc) = ctx.create_oscillator() else { return };
    osc.set_type(wave);
    osc.frequency().set_value(freq);

    let Ok(gain) = ctx.create_gain() else { return };
    let t = now(ctx);
    let g = gain.gain();
    g.set_value(0.0001);
    let _ = g.linear_ramp_to_value_at_time(peak, t + 0.003);
    let _ = g.exponential_ramp_to_value_at_time(0.0001, t + dur);

    // A gentle low-pass keeps the blip warm rather than piercing.
    let Ok(filter) = ctx.create_biquad_filter() else { return };
    filter.set_type(BiquadFilterType::Lowpass);
    filter.frequency().set_value(freq * 3.0);

    let _ = osc.connect_with_audio_node(&filter);
    let _ = filter.connect_with_audio_node(&gain);
    let _ = gain.connect_with_audio_node(&ctx.destination());
    let _ = osc.start();
    let _ = osc.stop_with_when(t + dur + 0.02);
    let _ = keep_osc(&osc, &filter, &gain);
}

/// Build (once) a mono white-noise buffer ~0.4 s long.
fn noise_buffer(ctx: &AudioContext) -> Option<AudioBuffer> {
    NOISE.with(|n| {
        if n.borrow().is_none() {
            let sample_rate = ctx.sample_rate();
            let len = (sample_rate * 0.4) as u32;
            let buf = ctx.create_buffer(1, len, sample_rate).ok()?;
            let mut data = vec![0.0f32; len as usize];
            // Cheap deterministic LCG noise — quality is irrelevant, we only
            // want broadband hiss to filter.
            let mut state: u32 = 0x1234_5678;
            for s in data.iter_mut() {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *s = (state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            }
            buf.copy_to_channel(&mut data, 0).ok()?;
            *n.borrow_mut() = Some(buf);
        }
        n.borrow().clone()
    })
}

// Web Audio nodes are cleaned up by the browser once they finish and go out of
// scope; these no-ops document that we intentionally drop our handles. (Keeping
// them alive isn't required — the graph holds its own references until `stop`.)
fn keep(_a: &BiquadFilterNode, _b: &GainNode) -> Option<()> {
    Some(())
}
fn keep_osc(_a: &OscillatorNode, _b: &BiquadFilterNode, _c: &GainNode) -> Option<()> {
    Some(())
}

// ---- Public sound events ------------------------------------------------

/// A single card sliding onto the felt: a brief, airy whisk.
pub fn deal_card() {
    if is_muted() {
        return;
    }
    if let Some(c) = ctx() {
        resume(&c);
        noise_burst(&c, 2600.0, 0.05, 0.14);
    }
}

/// Chips pushed in on a bet/raise: two soft muted ticks.
pub fn chip() {
    if is_muted() {
        return;
    }
    if let Some(c) = ctx() {
        resume(&c);
        blip(&c, 320.0, 0.05, 0.05, OscillatorType::Triangle);
        // A second, slightly higher tick a hair later for a "chips clink" feel.
        let c2 = c.clone();
        blip(&c2, 440.0, 0.035, 0.06, OscillatorType::Triangle);
    }
}

/// A light knuckle-tap on the felt for a check.
pub fn check_tap() {
    if is_muted() {
        return;
    }
    if let Some(c) = ctx() {
        resume(&c);
        noise_burst(&c, 900.0, 0.05, 0.07);
    }
}

/// Cards mucked into the pile on a fold: a shorter, duller whisk.
pub fn fold_swish() {
    if is_muted() {
        return;
    }
    if let Some(c) = ctx() {
        resume(&c);
        noise_burst(&c, 1500.0, 0.045, 0.11);
    }
}

/// The pot being raked to the winner: a soft descending pair of chip ticks.
pub fn pot_win() {
    if is_muted() {
        return;
    }
    if let Some(c) = ctx() {
        resume(&c);
        blip(&c, 520.0, 0.05, 0.09, OscillatorType::Sine);
        blip(&c.clone(), 390.0, 0.045, 0.12, OscillatorType::Sine);
        noise_burst(&c, 3000.0, 0.03, 0.18);
    }
}

/// Some browsers start the context suspended until a gesture; nudge it awake.
fn resume(ctx: &AudioContext) {
    if ctx.state() == web_sys::AudioContextState::Suspended {
        let _ = ctx.resume();
    }
}
