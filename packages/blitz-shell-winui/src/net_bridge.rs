use std::sync::Arc;
use windows::core::{IInspectable, Interface};
use crate::bindings::INetworkFetcher;
use blitz_net_winui::HostFetcher;
use crate::winrt_component::debug_log;

pub struct HostNetworkDispatcher {
    pub fetcher: IInspectable,
}

// The underlying WinRT IInspectable is apartment-threaded; we only call it on the UI thread.
// We mark this dispatcher Send+Sync to satisfy trait bounds but ensure actual usage stays on UI thread.
unsafe impl Send for HostNetworkDispatcher {}
unsafe impl Sync for HostNetworkDispatcher {}

impl HostFetcher for HostNetworkDispatcher {
    fn request_url(&self, doc_id: usize, url: &str, request_id: u32) -> bool {
        debug_log(&format!("HostNetworkDispatcher.request_url: req_id={} doc_id={} url={}", request_id, doc_id, url));
        if let Ok(f) = self.fetcher.cast::<INetworkFetcher>() {
            use windows::core::HSTRING;
            let url_h = HSTRING::from(url);
            let method_h = HSTRING::from("GET");
            let ok = f.Fetch(request_id, doc_id as u32, &url_h, &method_h).is_ok();
            if ok { debug_log(&format!("HostNetworkDispatcher.request_url: dispatched req_id={}", request_id)); }
            else { debug_log(&format!("HostNetworkDispatcher.request_url: Fetch call failed req_id={}", request_id)); }
            ok
        } else { debug_log("HostNetworkDispatcher.request_url: cast to INetworkFetcher failed"); false }
    }
}

pub fn make_provider(fetcher: IInspectable) -> Arc<blitz_net_winui::WinUiNetProvider<blitz_dom::net::Resource>> {
    let dispatcher = HostNetworkDispatcher { fetcher };
    blitz_net_winui::WinUiNetProvider::shared(Arc::new(dispatcher))
}
