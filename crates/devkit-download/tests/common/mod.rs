//! Shared fixture HTTP server for download tests
//! (docs/testing/testing-strategy.md: integration tests run a local
//! fixture server on loopback — real vendor hosts are never touched).
//!
//! The server threads are daemons; cargo test terminates the process, so
//! no explicit shutdown bookkeeping is needed.

use std::io::Read;
use std::time::Duration;

use tiny_http::{Request, Server};

/// Binds a fixture server on an ephemeral loopback port and serves every
/// incoming request through `handler` (which must respond to it).
pub fn spawn_fixture_server(handler: impl Fn(Request) + Send + 'static) -> u16 {
    let server = Server::http("127.0.0.1:0").expect("bind loopback fixture server");
    let port = match server.server_addr() {
        tiny_http::ListenAddr::IP(address) => address.port(),
        // On non-Unix builds `IP` is the only variant; the second arm is
        // unreachable there but required on Unix targets.
        #[allow(unreachable_patterns)]
        other => panic!("unexpected listen address: {other:?}"),
    };
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            handler(request);
        }
    });
    port
}

/// Deterministic test payload: bytes 0..=255 repeated to `len`.
pub fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index % 256) as u8).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Reader that trickles `chunks` chunks of ~16 KiB with a sleep between
/// reads — drives mid-download cancellation tests. Only some test binaries
/// that include this shared module use it.
#[allow(dead_code)]
pub struct TrickleReader {
    remaining_chunks: u32,
}

impl TrickleReader {
    #[allow(dead_code)]
    pub fn new(chunks: u32) -> Self {
        Self {
            remaining_chunks: chunks,
        }
    }
}

impl Read for TrickleReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining_chunks == 0 {
            return Ok(0);
        }
        self.remaining_chunks -= 1;
        let count = buf.len().min(16 * 1024);
        buf[..count].fill(0xab);
        std::thread::sleep(Duration::from_millis(30));
        Ok(count)
    }
}
