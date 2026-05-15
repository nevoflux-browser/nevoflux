//! Reassemble PNG frame bytes from 1 MB chunks streamed via the bridge.

use std::collections::HashMap;

/// Per-frame chunk state.
struct FrameState {
    total: u32,
    received: HashMap<u32, Vec<u8>>,
    is_last_seen: bool,
}

/// Holds partial frames until all chunks arrive, then yields the
/// concatenated PNG bytes.
pub struct ChunkBuffer {
    frames: HashMap<u32, FrameState>,
}

impl ChunkBuffer {
    pub fn new() -> Self {
        Self {
            frames: HashMap::new(),
        }
    }

    /// Returns `Some(png_bytes)` when the frame is fully assembled,
    /// otherwise `None`. Mismatched `total_chunks` between chunks of
    /// the same frame causes the incoming chunk to be silently dropped.
    pub fn add_chunk(
        &mut self,
        frame_idx: u32,
        chunk_idx: u32,
        total_chunks: u32,
        is_last: bool,
        bytes: Vec<u8>,
    ) -> Option<Vec<u8>> {
        let state = self.frames.entry(frame_idx).or_insert_with(|| FrameState {
            total: total_chunks,
            received: HashMap::new(),
            is_last_seen: false,
        });
        if state.total != total_chunks {
            // contradictory — drop
            return None;
        }
        if is_last {
            state.is_last_seen = true;
        }
        state.received.insert(chunk_idx, bytes);

        if state.received.len() as u32 == state.total && state.is_last_seen {
            // assemble
            let mut out = Vec::new();
            for i in 0..state.total {
                if let Some(c) = state.received.get(&i) {
                    out.extend_from_slice(c);
                } else {
                    return None; // missing chunk
                }
            }
            self.frames.remove(&frame_idx);
            return Some(out);
        }
        None
    }

    pub fn has_frame(&self, frame_idx: u32) -> bool {
        self.frames.contains_key(&frame_idx)
    }
}

impl Default for ChunkBuffer {
    fn default() -> Self {
        Self::new()
    }
}
