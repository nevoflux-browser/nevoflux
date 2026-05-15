// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Range-based port discovery for the AssetServer.
//!
//! The daemon's bridge already runs `find_available_port()` over the same
//! range; the AssetServer takes the next free slot in that range so
//! debugging stays predictable and two daemons co-existing on the same
//! machine each get distinct, deterministic ports.

use tokio::net::TcpListener;

use super::BindError;

/// Bind a TCP listener on the first free port in `range` (loopback only).
///
/// On success returns the listener already bound. On exhaustion returns
/// [`BindError::NoFreePortInRange`] with the range echoed back.
pub async fn bind_in_range(range: &std::ops::Range<u16>) -> Result<TcpListener, BindError> {
    for port in range.clone() {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            return Ok(listener);
        }
    }
    Err(BindError::NoFreePortInRange {
        start: range.start,
        end: range.end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn range_discovery_picks_first_free() {
        // First call binds the first free slot in a wide range; second call
        // (with the first listener still alive) picks a different port.
        let r = 19500..19601;
        let l1 = bind_in_range(&r).await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        assert!(p1 >= 19500 && p1 < 19601, "got out-of-range port {p1}");

        let l2 = bind_in_range(&r).await.unwrap();
        let p2 = l2.local_addr().unwrap().port();
        assert!(p2 >= 19500 && p2 < 19601);
        assert_ne!(p1, p2, "second binder got same port as first");
    }

    #[tokio::test]
    async fn range_exhausted_returns_error() {
        // A 1-wide range with the only port occupied must yield NoFreePortInRange.
        let r = 19500..19601;
        let _occupy = bind_in_range(&r).await.unwrap();
        let occupied_port = _occupy.local_addr().unwrap().port();

        let narrow = occupied_port..occupied_port + 1;
        let err = bind_in_range(&narrow).await.unwrap_err();
        match err {
            BindError::NoFreePortInRange { start, end } => {
                assert_eq!(start, occupied_port);
                assert_eq!(end, occupied_port + 1);
            }
            other => panic!("expected NoFreePortInRange, got {other:?}"),
        }
    }
}
