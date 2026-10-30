//! Background thread emitting `engine-status` (~30 Hz) and collecting retired graphs.

use std::thread;
use std::time::Duration;

use engine::model::MAX_TRACKS;
use engine::realtime::TransportStatus;
use engine::render::{MeterSnapshot, StereoLevel};
use tauri::{AppHandle, Emitter, Manager};

use crate::dto::{EVENT_ENGINE_STATUS, EngineStatusEvent, TrackLevel};
use crate::state::AppState;

/// About 30 Hz: smooth meters without flooding the IPC channel.
const STATUS_INTERVAL: Duration = Duration::from_millis(33);

pub fn spawn_status_pump(app: AppHandle) {
    let spawned = thread::Builder::new()
        .name("status-pump".into())
        .spawn(move || run(app));
    if let Err(err) = spawned {
        eprintln!("status pump not started: {err}");
    }
}

fn run(app: AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    // Last meter snapshot, re-sent while playing when the audio thread has not produced a new one.
    let mut held = MeterSnapshot::default();
    loop {
        thread::sleep(STATUS_INTERVAL);
        let Some((transport, fresh, error)) = poll_engine(&state) else {
            continue;
        };
        let event = compose_event(transport, fresh.as_ref(), &held, error);
        if let Some(meters) = fresh {
            held = meters;
        }
        // Emit fails only when the app is shutting down; the thread ends with the process.
        let _ = app.emit(EVENT_ENGINE_STATUS, event);
    }
}

/// Takes one reading from the engine under its lock. `None` when no device is open.
fn poll_engine(
    state: &AppState,
) -> Option<(TransportStatus, Option<MeterSnapshot>, Option<String>)> {
    let mut guard = state.lock_engine();
    let engine = guard.as_mut()?;
    engine.collect_garbage();
    let transport = engine.status();
    let fresh = engine.poll_meters();
    let error = engine.take_error();
    Some((transport, fresh, error))
}

/// Builds the event. Meters: fresh values when available; otherwise the held values while
/// playing, and silence while paused so meters fall to zero. Only fresh values carry `clipped`.
fn compose_event(
    transport: TransportStatus,
    fresh: Option<&MeterSnapshot>,
    held: &MeterSnapshot,
    error: Option<String>,
) -> EngineStatusEvent {
    let silent = silenced(held);
    let meters = match fresh {
        Some(meters) => meters,
        None if transport.playing => held,
        None => &silent,
    };
    let tracks = meters
        .track_ids
        .iter()
        .zip(&meters.tracks)
        .take(meters.track_count.min(MAX_TRACKS))
        .map(|(&id, &level)| TrackLevel { id, level })
        .collect();
    EngineStatusEvent {
        playing: transport.playing,
        position: transport.position,
        master: meters.master,
        tracks,
        clipped: fresh.is_some_and(|m| m.clipped),
        error,
    }
}

/// Same track layout as `meters`, every level zeroed.
fn silenced(meters: &MeterSnapshot) -> MeterSnapshot {
    MeterSnapshot {
        master: StereoLevel::default(),
        tracks: [StereoLevel::default(); MAX_TRACKS],
        clipped: false,
        ..*meters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(peak: f32) -> StereoLevel {
        StereoLevel {
            peak_l: peak,
            peak_r: peak,
            rms_l: peak / 2.0,
            rms_r: peak / 2.0,
        }
    }

    fn transport(playing: bool) -> TransportStatus {
        TransportStatus {
            playing,
            position: 480,
        }
    }

    /// Meters with the given master peak and `(track id, peak)` pairs.
    fn meters(master: f32, tracks: &[(u64, f32)], clipped: bool) -> MeterSnapshot {
        let mut track_levels = [StereoLevel::default(); MAX_TRACKS];
        let mut track_ids = [0; MAX_TRACKS];
        for (slot, (id, peak)) in tracks.iter().enumerate() {
            track_ids[slot] = *id;
            track_levels[slot] = level(*peak);
        }
        MeterSnapshot {
            master: level(master),
            tracks: track_levels,
            track_ids,
            track_count: tracks.len(),
            clipped,
        }
    }

    #[test]
    fn fresh_meters_are_used_and_carry_the_clip_flag() {
        let fresh = meters(0.9, &[(7, 0.4)], true);
        let held = meters(0.1, &[(7, 0.1)], false);

        let event = compose_event(transport(true), Some(&fresh), &held, None);

        assert_eq!(event.master, level(0.9));
        assert_eq!(event.tracks[0].level, level(0.4));
        assert!(event.clipped);
        assert!(event.playing);
        assert_eq!(event.position, 480);
    }

    #[test]
    fn held_meters_are_resent_while_playing_without_new_readings() {
        let held = meters(0.5, &[(3, 0.25)], true);

        let event = compose_event(transport(true), None, &held, None);

        assert_eq!(event.master, level(0.5));
        assert_eq!(event.tracks[0].id, 3);
        assert_eq!(event.tracks[0].level, level(0.25));
        assert!(!event.clipped, "clip flag belongs to fresh readings only");
    }

    #[test]
    fn paused_without_new_readings_sends_zero_levels_but_keeps_track_ids() {
        let held = meters(0.5, &[(3, 0.25), (4, 0.75)], false);

        let event = compose_event(transport(false), None, &held, None);

        assert_eq!(event.master, StereoLevel::default());
        let ids: Vec<u64> = event.tracks.iter().map(|t| t.id).collect();
        assert_eq!(ids, [3, 4]);
        assert!(
            event
                .tracks
                .iter()
                .all(|t| t.level == StereoLevel::default())
        );
    }

    #[test]
    fn track_list_stops_at_track_count_even_when_arrays_hold_stale_entries() {
        let held = meters(0.2, &[(1, 0.2)], false);
        let mut stale = held;
        stale.track_ids[5] = 99;

        let event = compose_event(transport(true), None, &stale, None);

        assert_eq!(event.tracks.len(), 1);
        assert_eq!(event.tracks[0].id, 1);
    }

    #[test]
    fn device_error_is_passed_through_once_per_event() {
        let held = meters(0.0, &[], false);

        let event = compose_event(transport(false), None, &held, Some("unplugged".into()));

        assert_eq!(event.error.as_deref(), Some("unplugged"));
        assert!(event.tracks.is_empty());
    }
}
