# Phase 6 — APU (Audio)

## Goal
Implement the four Game Boy sound channels, mix them, and play them through SDL with audio-driven pacing and no crackling.

## Prerequisites
Phase 5 gate met (or at least phase 4; this phase is independent of Pokémon compatibility).
Read Pan Docs: Audio Overview, Audio Registers, Audio Details (frame sequencer, length, envelope, sweep, wave RAM, noise LFSR).

## Architecture
- `apu/` module with one struct per channel (`Square1` with sweep, `Square2`, `Wave`, `Noise`) and a `FrameSequencer`.
- APU is ticked from `Bus::tick()` like the other components, deterministic, and produces samples into an internal
  ring buffer drained by `drain_audio()`.
- **Sample generation**: accumulate fractional cycles; emit one stereo sample every `4194304 / sample_rate` T-cycles
  (use 48000 Hz by default; make it configurable). Start with simple nearest/box sampling; improve filtering later.
- Frame sequencer: 512 Hz, driven from a **bit of the DIV counter** (falling edge of bit 12, or bit 13 in double speed),
  not from an independent counter. This is why phase 2 required DIV to be the top of a 16-bit counter.
  Steps: length (steps 0,2,4,6), sweep (2,6), envelope (7).

## Tasks
1. Registers NR10–NR52 and wave RAM (FF30–FF3F) with correct read masks (unused bits read 1), write behaviour,
   and the "APU off" state (NR52 bit 7: while off, registers except NR52 (and wave RAM, length on DMG) are read-only and zeroed).
2. Square channel with duty patterns, frequency timer `(2048 - freq) * 4`, volume envelope, length counter,
   trigger behaviour. Channel 1 adds the frequency sweep (with overflow disabling the channel).
3. Wave channel: 32 4-bit samples, volume shift, frequency timer `(2048 - freq) * 2`, trigger quirks.
4. Noise channel: LFSR with 7-bit/15-bit width mode, divisor table, clock shift.
5. Mixer: NR51 panning, NR50 master volume (and the DAC enable bit per channel; a channel with DAC off is silent
   and cannot be enabled by trigger).
6. High-pass filter (capacitor) to remove DC offset; simple and sufficient.
7. **Frontend audio**: SDL audio queue/callback fed from `drain_audio()`. Switch pacing from vsync to
   **audio-clock-driven** with dynamic rate control (adjust speed by a tiny factor to keep the queue near a target fill level).
   Document the target buffer size (e.g. ~50 ms).
8. Mute/channel toggle keys (1–4) for debugging; volume control.
9. Headless audio dump: `gb-cli --wav out.wav --frames N` writes a WAV so sound can be tested without a speaker.

## Known pitfalls
- Length counter and the frame sequencer interact at trigger time; edge cases are covered by Blargg's `dmg_sound` ROMs (see there).
- Zombie-mode envelope quirks, and wave channel read/write timing while playing, are obscure — implement the
  documented behaviour, test, and stop once Blargg passes what is reasonable.
- Clicks/crackle are almost always buffer underruns or discontinuities from resetting phase on register writes, not DSP bugs.
- Keep emulation deterministic: audio output must not influence emulated time except through the frontend's pacing, never inside `gb-core`.

## Gate (definition of done)

> **Status: done.** Gate evidence, owner waivers and measured results are recorded in `PROGRESS.md` (section "Phase 6"). The boxes below are the original checklist and were not ticked individually.
- [ ] Blargg `dmg_sound` individual ROMs: record pass/fail per ROM; target is all passing except documented obscure ones.
- [ ] WAV dumps from a test ROM or homebrew sound demo visibly (waveform) and audibly correct (human listens).
- [ ] Human confirms audio in Pokémon (title theme, battle music) sounds right and doesn't crackle on desktop and Steam Deck.
- [ ] Pacing is audio-driven and stays stable (no drift) over a 10-minute run.

## Out of scope
CGB-specific sound differences (revisit in phase 7/8), fancy resampling/filters.
