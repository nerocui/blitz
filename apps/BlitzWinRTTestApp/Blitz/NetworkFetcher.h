#pragma once

#include "NetworkFetcher.g.h"
#include <winrt/BlitzWinUI.h>
// Use dot-delimited C++/WinRT projection headers (directory style path was invalid)
#include <winrt/Windows.Web.Http.h>
#include <winrt/Windows.Storage.Streams.h>

namespace winrt::Blitz::implementation
{
    struct NetworkFetcher : NetworkFetcherT<NetworkFetcher>
    {
        NetworkFetcher(winrt::BlitzWinUI::Host const& host);

        void Fetch(uint32_t requestId, uint32_t docId, winrt::hstring const& url, winrt::hstring const& method);
    private:
        winrt::BlitzWinUI::Host m_host{ nullptr };
        winrt::Windows::Web::Http::HttpClient m_client{ nullptr };

        winrt::fire_and_forget DoFetch(uint32_t requestId, uint32_t docId, winrt::hstring url, winrt::hstring method);
    };
}

namespace winrt::Blitz::factory_implementation
{
    struct NetworkFetcher : NetworkFetcherT<NetworkFetcher, implementation::NetworkFetcher>
    {
    };
}
