using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.UI;
using MarkdownTest.Logging;

namespace MarkdownTest.Controls
{
    public sealed partial class LogViewer : UserControl
    {
        private ObservableCollection<LogEntry> _logs = new ObservableCollection<LogEntry>();
        private HashSet<string> _categories = new HashSet<string>();
        private HashSet<string> _locations = new HashSet<string>();
        private Guid _logSubscriptionId;
        private bool _initialized = false;

        public LogViewer()
        {
            this.InitializeComponent();
            
            // Remove the resource additions that are causing errors
            // The resources are already defined in XAML
            
            lvLogs.ItemsSource = _logs;
        }

        public void Initialize()
        {
            if (_initialized)
                return;
            
            _initialized = true;
            
            // Subscribe to new logs
            _logSubscriptionId = LogManager.Instance.Subscribe(OnNewLog);
            
            // Load existing logs
            var existingLogs = LogManager.Instance.GetLogs(maxResults: 1000).ToList();
            foreach (var log in existingLogs.OrderBy(l => l.Timestamp))
            {
                AddLogEntry(log, false);
            }
            
            // Scroll to the bottom initially
            if (_logs.Count > 0)
            {
                lvLogs.SelectedIndex = _logs.Count - 1;
                lvLogs.ScrollIntoView(lvLogs.SelectedItem);
            }
        }

        private void OnNewLog(LogEntry log)
        {
            // We need to use dispatcher since this callback might be on a different thread
            this.DispatcherQueue.TryEnqueue(() =>
            {
                AddLogEntry(log, tsAutoScroll.IsOn);
            });
        }

        private void AddLogEntry(LogEntry log, bool scrollToBottom)
        {
            // Add log to our collection
            _logs.Add(log);
            
            // Update filter collections
            if (!string.IsNullOrEmpty(log.Category) && !_categories.Contains(log.Category))
            {
                _categories.Add(log.Category);
                UpdateCategoryFilter();
            }
            
            if (!string.IsNullOrEmpty(log.Location) && !_locations.Contains(log.Location))
            {
                _locations.Add(log.Location);
                UpdateLocationFilter();
            }
            
            // Scroll to the new log if auto-scroll is enabled
            if (scrollToBottom)
            {
                lvLogs.SelectedIndex = _logs.Count - 1;
                lvLogs.ScrollIntoView(lvLogs.SelectedItem);
            }
        }

        private void UpdateCategoryFilter()
        {
            var currentValue = cbCategory.SelectedItem as string;
            
            cbCategory.Items.Clear();
            cbCategory.Items.Add("All Categories");
            
            foreach (var category in _categories.OrderBy(c => c))
            {
                cbCategory.Items.Add(category);
            }
            
            if (currentValue != null && cbCategory.Items.Contains(currentValue))
            {
                cbCategory.SelectedItem = currentValue;
            }
            else
            {
                cbCategory.SelectedIndex = 0; // "All Categories"
            }
        }

        private void UpdateLocationFilter()
        {
            var currentValue = cbLocation.SelectedItem as string;
            
            cbLocation.Items.Clear();
            cbLocation.Items.Add("All Locations");
            
            foreach (var location in _locations.OrderBy(l => l))
            {
                cbLocation.Items.Add(location);
            }
            
            if (currentValue != null && cbLocation.Items.Contains(currentValue))
            {
                cbLocation.SelectedItem = currentValue;
            }
            else
            {
                cbLocation.SelectedIndex = 0; // "All Locations"
            }
        }

        private void Filter_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            ApplyFilters();
        }

        private void ApplyFilters()
        {
            // Get filter values
            string categoryFilter = cbCategory.SelectedItem as string;
            string locationFilter = cbLocation.SelectedItem as string;
            
            // If both are "All", get all logs
            if ((categoryFilter == null || categoryFilter == "All Categories") &&
                (locationFilter == null || locationFilter == "All Locations"))
            {
                RefreshLogs(null);
                return;
            }
            
            // Build query based on filters
            Func<LogEntry, bool> filter = (log) =>
            {
                bool categoryMatch = categoryFilter == null || 
                                    categoryFilter == "All Categories" || 
                                    log.Category == categoryFilter;
                
                bool locationMatch = locationFilter == null || 
                                    locationFilter == "All Locations" || 
                                    log.Location == locationFilter;
                
                return categoryMatch && locationMatch;
            };
            
            RefreshLogs(filter);
        }

        private void RefreshLogs(Func<LogEntry, bool> filter)
        {
            // Get filtered logs
            var logs = LogManager.Instance.GetLogs(maxResults: 1000);
            
            if (filter != null)
            {
                logs = logs.Where(filter);
            }
            
            // Update the collection
            _logs.Clear();
            foreach (var log in logs.OrderBy(l => l.Timestamp))
            {
                _logs.Add(log);
            }
            
            // Scroll to bottom
            if (_logs.Count > 0 && tsAutoScroll.IsOn)
            {
                lvLogs.SelectedIndex = _logs.Count - 1;
                lvLogs.ScrollIntoView(lvLogs.SelectedItem);
            }
        }

        private void Timestamps_Toggled(object sender, RoutedEventArgs e)
        {
            // This is handled by binding in XAML
        }

        private void BtnClear_Click(object sender, RoutedEventArgs e)
        {
            // Clear the displayed logs (not the actual log storage)
            _logs.Clear();
            tbSelectionDetails.Text = "Select logs to view details or compare timestamps";
            tbTimestampDelta.Visibility = Visibility.Collapsed;
        }

        private async void BtnExport_Click(object sender, RoutedEventArgs e)
        {
            var savePicker = new FileSavePicker();
            savePicker.SuggestedStartLocation = PickerLocationId.DocumentsLibrary;
            savePicker.FileTypeChoices.Add("Text Files", new List<string>() { ".txt" });
            savePicker.SuggestedFileName = $"Logs_{DateTime.Now:yyyyMMdd_HHmmss}";
            
            // Get current window handle to initialize the picker
            var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(Window.Current);
            WinRT.Interop.InitializeWithWindow.Initialize(savePicker, hwnd);
            
            StorageFile file = await savePicker.PickSaveFileAsync();
            if (file != null)
            {
                try
                {
                    // Build filter if any is applied
                    Func<LogEntry, bool> filter = null;
                    string categoryFilter = cbCategory.SelectedItem as string;
                    string locationFilter = cbLocation.SelectedItem as string;
                    
                    if ((categoryFilter != null && categoryFilter != "All Categories") ||
                        (locationFilter != null && locationFilter != "All Locations"))
                    {
                        filter = (log) =>
                        {
                            bool categoryMatch = categoryFilter == null || 
                                                categoryFilter == "All Categories" || 
                                                log.Category == categoryFilter;
                            
                            bool locationMatch = locationFilter == null || 
                                                locationFilter == "All Locations" || 
                                                log.Location == locationFilter;
                            
                            return categoryMatch && locationMatch;
                        };
                    }
                    
                    // Export logs (this uses the LogManager's export functionality)
                    await LogManager.Instance.ExportLogsToFileAsync(file.Path, filter);
                    
                    // Show confirmation
                    ContentDialog dialog = new ContentDialog()
                    {
                        Title = "Export Successful",
                        Content = $"Logs exported to {file.Name}",
                        CloseButtonText = "OK",
                        XamlRoot = this.XamlRoot
                    };
                    
                    await dialog.ShowAsync();
                }
                catch (Exception ex)
                {
                    // Show error
                    ContentDialog dialog = new ContentDialog()
                    {
                        Title = "Export Failed",
                        Content = $"Error: {ex.Message}",
                        CloseButtonText = "OK",
                        XamlRoot = this.XamlRoot
                    };
                    
                    await dialog.ShowAsync();
                }
            }
        }

        private void LvLogs_ItemClick(object sender, ItemClickEventArgs e)
        {
            if (e.ClickedItem is LogEntry log)
            {
                // Show details of clicked log
                tbSelectionDetails.Text = $"Log #{log.Id}: {log.Timestamp:HH:mm:ss.fff} [{log.Category}] [{log.Location}] - {log.Message}";
            }
        }

        private void LvLogs_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            var selectedItems = lvLogs.SelectedItems.Cast<LogEntry>().ToList();
            
            if (selectedItems.Count == 2)
            {
                // Calculate time difference between two selected logs
                var log1 = selectedItems[0];
                var log2 = selectedItems[1];
                
                // Ensure log1 is the earlier one
                if (log1.Timestamp > log2.Timestamp)
                {
                    var temp = log1;
                    log1 = log2;
                    log2 = temp;
                }
                
                TimeSpan diff = log2.Timestamp - log1.Timestamp;
                
                tbTimestampDelta.Text = $"Time between log #{log1.Id} and #{log2.Id}: {diff.TotalMilliseconds:F3} ms";
                tbTimestampDelta.Visibility = Visibility.Visible;
            }
            else
            {
                tbTimestampDelta.Visibility = Visibility.Collapsed;
            }
        }
    }

    public class TimestampConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            if (value is DateTime dateTime)
            {
                return dateTime.ToString("HH:mm:ss.fff");
            }
            return value;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }

    public class BoolToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            if (value is bool boolValue)
            {
                return boolValue ? Visibility.Visible : Visibility.Collapsed;
            }
            return Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }

    public class CountToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, string language)
        {
            if (value is int count)
            {
                return count == 0 ? Visibility.Visible : Visibility.Collapsed;
            }
            return Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, string language)
        {
            throw new NotImplementedException();
        }
    }
}
