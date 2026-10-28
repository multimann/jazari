//! Graphic symbol playback: which frame of a graphic's own timeline shows at
//! a frame of its parent.
//!
//! Flash flattens graphic symbols into the parent timeline (research F23,
//! confirmed by the `tweens` fixture, where the 10-frame `Counter` graphic
//! becomes 10 shapes swapped per frame). The inner frame depends only on the
//! playback mode, `first_frame`, the symbol's length and the number of
//! frames since the keyframe that placed the instance.

use jazari_xfl::model::{GraphicPlayback, LoopMode};

/// The 0-based inner frame shown `elapsed` frames after the keyframe that
/// placed a graphic instance, for a symbol of `len` frames. `None` when the
/// symbol has no frames.
///
/// - loop: `(first_frame + elapsed) mod len`;
/// - play once: `min(first_frame + elapsed, len − 1)`;
/// - single frame: `first_frame`.
///
/// A `first_frame` past the end is clamped to the last frame (single frame,
/// play once) or wrapped (loop).
///
/// Inside a motion tween `elapsed` counts from the tween's start keyframe
/// with that keyframe's settings; each keyframe (the tween's end included)
/// restarts with its own loop mode and `first_frame`. The
/// `tween-probes/graphics` probe confirms this for every mode, with the
/// tween's "sync" option on and off: the IDE already stores the continued
/// `first_frame` in the following keyframe, so sync needs nothing here.
pub fn inner_frame(playback: GraphicPlayback, len: u32, elapsed: u32) -> Option<u32> {
    if len == 0 {
        return None;
    }
    let first = u64::from(playback.first_frame);
    let elapsed = u64::from(elapsed);
    let len64 = u64::from(len);
    let f = match playback.mode {
        LoopMode::Loop => (first + elapsed) % len64,
        LoopMode::PlayOnce => (first + elapsed).min(len64 - 1),
        LoopMode::SingleFrame => first.min(len64 - 1),
    };
    // f < len, which is a u32.
    u32::try_from(f).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pb(mode: LoopMode, first_frame: u32) -> GraphicPlayback {
        GraphicPlayback { mode, first_frame }
    }

    #[test]
    fn loop_wraps_from_the_first_frame() {
        let p = pb(LoopMode::Loop, 7);
        let frames: Vec<_> = (0..5).filter_map(|e| inner_frame(p, 10, e)).collect();
        assert_eq!(frames, [7, 8, 9, 0, 1]);
        assert_eq!(inner_frame(pb(LoopMode::Loop, 0), 10, 23), Some(3));
    }

    #[test]
    fn play_once_holds_the_last_frame() {
        let p = pb(LoopMode::PlayOnce, 3);
        let frames: Vec<_> = (0..9).filter_map(|e| inner_frame(p, 10, e)).collect();
        assert_eq!(frames, [3, 4, 5, 6, 7, 8, 9, 9, 9]);
    }

    #[test]
    fn single_frame_stays() {
        let p = pb(LoopMode::SingleFrame, 6);
        assert_eq!(inner_frame(p, 10, 0), Some(6));
        assert_eq!(inner_frame(p, 10, 50), Some(6));
        assert_eq!(inner_frame(pb(LoopMode::SingleFrame, 60), 10, 0), Some(9));
    }

    #[test]
    fn extremes_do_not_overflow() {
        assert_eq!(
            inner_frame(pb(LoopMode::Loop, u32::MAX), 7, u32::MAX),
            Some(((u64::from(u32::MAX) * 2) % 7) as u32)
        );
        assert_eq!(
            inner_frame(pb(LoopMode::PlayOnce, u32::MAX), 7, u32::MAX),
            Some(6)
        );
        assert_eq!(inner_frame(pb(LoopMode::Loop, 0), 0, 3), None);
    }
}
