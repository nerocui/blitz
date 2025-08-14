#include "pch.h"
#include "Attacher.h"
#if __has_include("Attacher.g.cpp")
#include "Attacher.g.cpp"
#endif

using namespace winrt;
using namespace winrt::Microsoft::UI::Xaml::Controls;

namespace winrt::Blitz::implementation
{
	Attacher::Attacher(winrt::Windows::Foundation::IInspectable const& panel)
	{
		if (panel)
		{
			try
			{
				m_panel = panel.as<SwapChainPanel>();
				::OutputDebugStringW(L"Attacher::Attacher: captured SwapChainPanel\n");
			}
			catch (...)
			{
				::OutputDebugStringW(L"Attacher::Attacher: panel is not a SwapChainPanel\n");
			}
		}
		else
		{
			::OutputDebugStringW(L"Attacher::Attacher: null panel provided\n");
		}
	}

	void Attacher::AttachSwapChain(uint64_t swapchainPtr)
	{
		m_lastSwapchainPtr = swapchainPtr;

		if (swapchainPtr == 0)
		{
			::OutputDebugStringW(L"Attacher::AttachSwapChain: null pointer, ignoring\n");
			return;
		}

		// C# demo used a sentinel test pointer; replicate optional ignore (same value)
		if (swapchainPtr == 0xFEEDFACECAFEBEEFULL)
		{
			::OutputDebugStringW(L"Attacher::AttachSwapChain: test pointer, ignoring\n");
			return;
		}

		if (!m_panel)
		{
			::OutputDebugStringW(L"Attacher::AttachSwapChain: panel not set\n");
			return;
		}

		struct __declspec(uuid("63AAD0B8-7C24-40FF-85A8-640D944CC325")) ISwapChainPanelNative : ::IUnknown
		{
			virtual HRESULT __stdcall SetSwapChain(::IUnknown* value) = 0;
		};

		auto panelUnknown = reinterpret_cast<::IUnknown*>(get_abi(m_panel));
		com_ptr<ISwapChainPanelNative> native;
		HRESULT qi = panelUnknown->QueryInterface(__uuidof(ISwapChainPanelNative), native.put_void());
		if (FAILED(qi))
		{
			::OutputDebugStringW(L"Attacher::AttachSwapChain: QI for ISwapChainPanelNative failed\n");
			return;
		}

		auto swapUnknown = reinterpret_cast<::IUnknown*>(swapchainPtr);
		HRESULT hr = native->SetSwapChain(swapUnknown);
		if (FAILED(hr))
		{
			wchar_t buf[160];
			swprintf_s(buf, L"Attacher::AttachSwapChain: SetSwapChain failed hr=0x%08X\n", hr);
			::OutputDebugStringW(buf);
		}
		else
		{
			::OutputDebugStringW(L"Attacher::AttachSwapChain: success\n");
		}
	}

	bool Attacher::TestAttacherConnection()
	{
		return true;
	}
}
