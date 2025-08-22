#include "pch.h"
#include "NetworkFetcher.h"
#if __has_include("NetworkFetcher.g.cpp")
#include "NetworkFetcher.g.cpp"
#endif
#include <string>
#include <vector>
#include <windows.h>

using namespace winrt;
using namespace winrt::Windows::Foundation;
using namespace winrt::Windows::Storage::Streams;
using namespace winrt::Windows::Web::Http;

namespace
{
    void LogUrl(winrt::hstring const& url)
    {
        std::wstring_view v = url;
        std::wstring line = L"[Fetch] URL '" + std::wstring(v) + L"' (len=" + std::to_wstring(v.size()) + L")\n";
        OutputDebugStringW(line.c_str());
    }
}

namespace winrt::Blitz::implementation
{
    NetworkFetcher::NetworkFetcher(winrt::BlitzWinUI::Host const& host)
        : m_host(host)
    {
        m_client = HttpClient();
    }

    void NetworkFetcher::Fetch(uint32_t requestId, uint32_t docId, winrt::hstring const& url, winrt::hstring const& method)
    {
        auto urlCopy = url; (void)method; // only GET for now
        DoFetch(requestId, docId, std::move(urlCopy), L"GET");
    }

    winrt::fire_and_forget NetworkFetcher::DoFetch(uint32_t requestId, uint32_t docId, winrt::hstring url, winrt::hstring method)
    {
        auto lifetime = get_strong(); (void)method;
        LogUrl(url);
        try
        {
            Uri uri(url);
            HttpResponseMessage response = co_await m_client.GetAsync(uri);
            response.EnsureSuccessStatusCode();
            IBuffer buffer = co_await response.Content().ReadAsBufferAsync();
            DataReader reader = DataReader::FromBuffer(buffer);
            std::vector<uint8_t> bytes(buffer.Length());
            reader.ReadBytes(bytes);
            if (m_host)
            {
                m_host.CompleteFetch(requestId, docId, true, bytes, L"");
            }
        }
        catch (hresult_error const& e)
        {
            if (m_host)
            {
                m_host.CompleteFetch(requestId, docId, false, {}, e.message());
            }
            std::wstring msg = L"[Fetch] FAILED: ";
            msg += e.message().c_str();
            msg += L"\n";
            OutputDebugStringW(msg.c_str());
        }
        co_return;
    }
}
