using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System;
using System.Collections.ObjectModel;
using MarkdownTest.Logging;

namespace MarkdownTest.Controls
{
    public sealed partial class DevToolsPanel : UserControl
    {
        public ObservableCollection<TabItem> Tabs { get; } = new ObservableCollection<TabItem>();
        
        private PerformanceMonitor _performanceMonitor;
        private LogViewer _logViewer;

        public DevToolsPanel()
        {
            this.InitializeComponent();
            
            // Create tab content
            _performanceMonitor = new PerformanceMonitor();
            _logViewer = new LogViewer();
            
            // Add tabs
            Tabs.Add(new TabItem { Header = "Performance", Content = _performanceMonitor });
            Tabs.Add(new TabItem { Header = "Logs", Content = _logViewer });
                        
            // Initialize log viewer with the logger instance
            _logViewer.Initialize();
            
            // Set the height after the panel is loaded, using the actual window size if available
        }

        private void BtnClose_Click(object sender, RoutedEventArgs e)
        {
            this.Visibility = Visibility.Collapsed;
        }
        
        public void UpdatePerformanceData(string performanceData)
        {
            _performanceMonitor.UpdatePerformanceData(performanceData);
        }
    }
    
    public class TabItem
    {
        public string Header { get; set; }
        public object Content { get; set; }
    }
}
