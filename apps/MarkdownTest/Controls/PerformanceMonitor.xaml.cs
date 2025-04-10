using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace MarkdownTest.Controls
{
    public sealed partial class PerformanceMonitor : UserControl
    {
        public PerformanceMonitor()
        {
            this.InitializeComponent();
        }
        
        public void UpdatePerformanceData(string performanceData)
        {
            if (string.IsNullOrEmpty(performanceData))
            {
                tbPerformanceData.Text = "No performance data available";
            }
            else
            {
                tbPerformanceData.Text = performanceData;
            }
        }
    }
}
