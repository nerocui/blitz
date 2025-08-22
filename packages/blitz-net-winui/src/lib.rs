//! WinUI host-driven networking provider placeholder.
//! This crate defines a NetProvider implementation that delegates actual network IO to a
//! host-provided WinRT INetworkFetcher (see blitz-shell-winui IDL). It focuses on request ID
//! tracking and mapping handler completions.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};
use blitz_traits::net::{NetProvider, Request, BoxedHandler};

// Lightweight logging hook (the shell exposes debug_log; we gate behind feature-less fn pointer lookup).
#[inline(always)]
fn host_debug_log(msg: &str) {
    extern "C" { fn __blitz_host_debug_log(ptr: *const u8, len: usize); }
    unsafe { let _ = std::panic::catch_unwind(|| __blitz_host_debug_log(msg.as_ptr(), msg.len())); }
}

// Trait the shell implements to let the provider ask the host to start a fetch.
pub trait HostFetcher: Send + Sync {
    // Return true if dispatch accepted; false if host not ready.
    fn request_url(&self, doc_id: usize, url: &str, request_id: u32) -> bool;
}

pub struct WinUiNetProvider<D: 'static> {
    host: Arc<dyn HostFetcher>,
    next_id: AtomicU32,
    // request_id -> (doc_id, handler)
    pending: Mutex<HashMap<u32, (usize, BoxedHandler<D>)>>,
}

impl<D: 'static> WinUiNetProvider<D> {
    pub fn new(host: Arc<dyn HostFetcher>) -> Self {
    host_debug_log("WinUiNetProvider: created");
    Self { host, next_id: AtomicU32::new(1), pending: Mutex::new(HashMap::new()) }
    }

    pub fn shared(host: Arc<dyn HostFetcher>) -> Arc<Self> { Arc::new(Self::new(host)) }

    pub fn take_handler(&self, id: u32) -> Option<(usize, BoxedHandler<D>)> {
        self.pending.lock().ok().and_then(|mut m| m.remove(&id))
    }
}

impl<D: 'static> NetProvider<D> for WinUiNetProvider<D> {
    fn fetch(&self, doc_id: usize, request: Request, handler: BoxedHandler<D>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let url_str = request.url.as_str().to_string();
        let pending_len = {
            let mut guard_opt = self.pending.lock().ok();
            if let Some(ref mut guard) = guard_opt { guard.insert(id, (doc_id, handler)); guard.len() } else { 0 }
        };
        host_debug_log(&format!("WinUiNetProvider.fetch: id={} doc_id={} url={} pending={} (dispatching)", id, doc_id, url_str, pending_len));
        if !self.host.request_url(doc_id, &url_str, id) {
            // Host rejected; remove handler and (best-effort) drop silently. Upstream can add error callback here.
            let _ = self.take_handler(id);
            host_debug_log(&format!("WinUiNetProvider.fetch: id={} rejected by host", id));
        }
    }
}
