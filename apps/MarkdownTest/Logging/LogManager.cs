using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;

namespace MarkdownTest.Logging
{
    /// <summary>
    /// Central manager for logging functionality
    /// </summary>
    public class LogManager
    {
        #region Singleton Pattern

        private static readonly Lazy<LogManager> _instance = new Lazy<LogManager>(() => new LogManager());
        
        /// <summary>
        /// Gets the singleton instance of the LogManager
        /// </summary>
        public static LogManager Instance => _instance.Value;

        #endregion

        #region Fields

        private int _nextLogId = 1;
        private readonly ConcurrentDictionary<int, LogEntry> _logEntries = new ConcurrentDictionary<int, LogEntry>();
        private readonly HashSet<string> _enabledCategories = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        private readonly HashSet<string> _enabledLocations = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        private readonly Dictionary<Guid, LogSubscription> _subscriptions = new Dictionary<Guid, LogSubscription>();
        private readonly object _subscriptionsLock = new object();
        private string _logFilePath = null;
        private StreamWriter _logFileWriter = null;
        private readonly object _fileWriterLock = new object();
        private readonly int _maxInMemoryEntries = 10000;
        private readonly ConcurrentQueue<int> _entryIdQueue = new ConcurrentQueue<int>();

        #endregion

        #region Constructor

        /// <summary>
        /// Creates a new LogManager instance
        /// </summary>
        private LogManager()
        {
            // Enable all categories and locations by default
            _enabledCategories.Add("*");
            _enabledLocations.Add("*");
        }

        #endregion

        #region Category and Location Management

        /// <summary>
        /// Enables a specific category for logging
        /// </summary>
        public void EnableCategory(string category)
        {
            if (!string.IsNullOrEmpty(category))
            {
                lock (_enabledCategories)
                {
                    _enabledCategories.Add(category);
                }
            }
        }

        /// <summary>
        /// Disables a specific category for logging
        /// </summary>
        public void DisableCategory(string category)
        {
            if (!string.IsNullOrEmpty(category))
            {
                lock (_enabledCategories)
                {
                    _enabledCategories.Remove(category);
                    // Keep the wildcard if that's not what we're removing
                    if (category != "*" && _enabledCategories.Count == 0)
                    {
                        _enabledCategories.Add("*");
                    }
                }
            }
        }

        /// <summary>
        /// Checks if a category is enabled for logging
        /// </summary>
        public bool IsCategoryEnabled(string category)
        {
            if (string.IsNullOrEmpty(category))
            {
                return true;
            }

            lock (_enabledCategories)
            {
                return _enabledCategories.Contains("*") || _enabledCategories.Contains(category);
            }
        }

        /// <summary>
        /// Enables a specific location for logging
        /// </summary>
        public void EnableLocation(string location)
        {
            if (!string.IsNullOrEmpty(location))
            {
                lock (_enabledLocations)
                {
                    _enabledLocations.Add(location);
                }
            }
        }

        /// <summary>
        /// Disables a specific location for logging
        /// </summary>
        public void DisableLocation(string location)
        {
            if (!string.IsNullOrEmpty(location))
            {
                lock (_enabledLocations)
                {
                    _enabledLocations.Remove(location);
                    // Keep the wildcard if that's not what we're removing
                    if (location != "*" && _enabledLocations.Count == 0)
                    {
                        _enabledLocations.Add("*");
                    }
                }
            }
        }

        /// <summary>
        /// Checks if a location is enabled for logging
        /// </summary>
        public bool IsLocationEnabled(string location)
        {
            if (string.IsNullOrEmpty(location))
            {
                return true;
            }

            lock (_enabledLocations)
            {
                return _enabledLocations.Contains("*") || _enabledLocations.Contains(location);
            }
        }

        #endregion

        #region Logging Methods

        /// <summary>
        /// Logs a message with the specified category and location
        /// </summary>
        public LogEntry Log(string message, string category = "General", string location = "C#")
        {
            if (!IsCategoryEnabled(category) || !IsLocationEnabled(location))
            {
                return null; // Skip logging if category or location is disabled
            }

            int id = Interlocked.Increment(ref _nextLogId);
            var entry = new LogEntry(id, message, category, location, DateTime.Now);
            
            // Store in memory
            _logEntries[id] = entry;
            _entryIdQueue.Enqueue(id);
            
            // Trim if we exceed the max entries
            while (_entryIdQueue.Count > _maxInMemoryEntries && _entryIdQueue.TryDequeue(out int oldestId))
            {
                _logEntries.TryRemove(oldestId, out _);
            }
            
            // Write to file if configured
            WriteToFileIfEnabled(entry);
            
            // Notify subscribers
            NotifySubscribers(entry);
            
            // Also output to debug console
            System.Diagnostics.Debug.WriteLine(entry.ToString());
            
            return entry;
        }

        /// <summary>
        /// Enable logging to a file
        /// </summary>
        public void EnableFileLogging(string filePath)
        {
            lock (_fileWriterLock)
            {
                // Close existing writer if any
                _logFileWriter?.Close();
                _logFileWriter?.Dispose();
                
                _logFilePath = filePath;
                
                try
                {
                    // Create directory if it doesn't exist
                    string directory = Path.GetDirectoryName(filePath);
                    if (!string.IsNullOrEmpty(directory) && !Directory.Exists(directory))
                    {
                        Directory.CreateDirectory(directory);
                    }
                    
                    // Open file for appending
                    _logFileWriter = new StreamWriter(filePath, true);
                    _logFileWriter.AutoFlush = true;
                    
                    // Write a header 
                    _logFileWriter.WriteLine($"--- Log started at {DateTime.Now} ---");
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error setting up log file: {ex.Message}");
                    _logFileWriter = null;
                    _logFilePath = null;
                }
            }
        }

        /// <summary>
        /// Disable logging to a file
        /// </summary>
        public void DisableFileLogging()
        {
            lock (_fileWriterLock)
            {
                if (_logFileWriter != null)
                {
                    _logFileWriter.WriteLine($"--- Log ended at {DateTime.Now} ---");
                    _logFileWriter.Close();
                    _logFileWriter.Dispose();
                    _logFileWriter = null;
                }
                _logFilePath = null;
            }
        }

        /// <summary>
        /// Export in-memory logs to a file
        /// </summary>
        public async Task ExportLogsToFileAsync(string filePath, Func<LogEntry, bool> filter = null)
        {
            try
            {
                string directory = Path.GetDirectoryName(filePath);
                if (!string.IsNullOrEmpty(directory) && !Directory.Exists(directory))
                {
                    Directory.CreateDirectory(directory);
                }
                
                using (var writer = new StreamWriter(filePath, false))
                {
                    await writer.WriteLineAsync($"--- Exported logs at {DateTime.Now} ---");
                    
                    var logs = filter != null 
                        ? _logEntries.Values.Where(filter).OrderBy(l => l.Id)
                        : _logEntries.Values.OrderBy(l => l.Id);
                    
                    foreach (var entry in logs)
                    {
                        await writer.WriteLineAsync(entry.ToString());
                    }
                    
                    await writer.WriteLineAsync($"--- End of exported logs ---");
                }
                
                return;
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Error exporting logs: {ex.Message}");
                throw;
            }
        }

        private void WriteToFileIfEnabled(LogEntry entry)
        {
            lock (_fileWriterLock)
            {
                if (_logFileWriter != null)
                {
                    try
                    {
                        _logFileWriter.WriteLine(entry.ToString());
                    }
                    catch (Exception ex)
                    {
                        System.Diagnostics.Debug.WriteLine($"Error writing to log file: {ex.Message}");
                        // Don't disable file logging here, just report the error
                    }
                }
            }
        }

        #endregion

        #region Query Methods

        /// <summary>
        /// Get a specific log entry by ID
        /// </summary>
        public LogEntry GetLogById(int id)
        {
            if (_logEntries.TryGetValue(id, out LogEntry entry))
            {
                return entry;
            }
            return null;
        }

        /// <summary>
        /// Get log entries with optional filtering
        /// </summary>
        public IEnumerable<LogEntry> GetLogs(
            string[] categories = null,
            string[] locations = null, 
            DateTime? startTime = null,
            DateTime? endTime = null,
            int? maxResults = null)
        {
            var query = _logEntries.Values.AsEnumerable();
            
            // Apply filters
            if (categories != null && categories.Length > 0)
            {
                query = query.Where(log => categories.Contains(log.Category, StringComparer.OrdinalIgnoreCase));
            }
            
            if (locations != null && locations.Length > 0)
            {
                query = query.Where(log => locations.Contains(log.Location, StringComparer.OrdinalIgnoreCase));
            }
            
            if (startTime.HasValue)
            {
                query = query.Where(log => log.Timestamp >= startTime.Value);
            }
            
            if (endTime.HasValue)
            {
                query = query.Where(log => log.Timestamp <= endTime.Value);
            }
            
            // Order by timestamp
            query = query.OrderByDescending(log => log.Timestamp);
            
            // Limit results if specified
            if (maxResults.HasValue && maxResults.Value > 0)
            {
                query = query.Take(maxResults.Value);
            }
            
            return query.ToList(); // Create a copy to avoid enumeration issues
        }

        /// <summary>
        /// Clear all in-memory logs
        /// </summary>
        public void ClearLogs()
        {
            _logEntries.Clear();
            while (_entryIdQueue.TryDequeue(out _)) { } // Clear the queue
        }

        #endregion

        #region Subscription Methods

        /// <summary>
        /// Subscribe to log entries with optional filtering
        /// </summary>
        public Guid Subscribe(Action<LogEntry> callback, string[] categories = null, string[] locations = null)
        {
            if (callback == null)
            {
                throw new ArgumentNullException(nameof(callback));
            }
            
            var subscription = new LogSubscription
            {
                Id = Guid.NewGuid(),
                Callback = callback,
                Categories = categories,
                Locations = locations
            };
            
            lock (_subscriptionsLock)
            {
                _subscriptions[subscription.Id] = subscription;
            }
            
            return subscription.Id;
        }

        /// <summary>
        /// Unsubscribe from log notifications
        /// </summary>
        public bool Unsubscribe(Guid subscriptionId)
        {
            lock (_subscriptionsLock)
            {
                return _subscriptions.Remove(subscriptionId);
            }
        }

        private void NotifySubscribers(LogEntry entry)
        {
            List<LogSubscription> subscriptionsToNotify = null;
            
            lock (_subscriptionsLock)
            {
                // Make a copy of subscriptions that match this entry's criteria
                subscriptionsToNotify = _subscriptions.Values
                    .Where(sub => MatchesSubscription(entry, sub))
                    .ToList();
            }
            
            // Notify outside the lock to avoid deadlocks
            foreach (var subscription in subscriptionsToNotify)
            {
                try
                {
                    subscription.Callback(entry);
                }
                catch (Exception ex)
                {
                    System.Diagnostics.Debug.WriteLine($"Error in log subscription callback: {ex.Message}");
                }
            }
        }

        private bool MatchesSubscription(LogEntry entry, LogSubscription subscription)
        {
            // If no categories specified, match all categories
            bool categoryMatch = subscription.Categories == null || subscription.Categories.Length == 0 ||
                                subscription.Categories.Contains(entry.Category, StringComparer.OrdinalIgnoreCase);
            
            // If no locations specified, match all locations
            bool locationMatch = subscription.Locations == null || subscription.Locations.Length == 0 ||
                                subscription.Locations.Contains(entry.Location, StringComparer.OrdinalIgnoreCase);
            
            return categoryMatch && locationMatch;
        }

        private class LogSubscription
        {
            public Guid Id { get; set; }
            public Action<LogEntry> Callback { get; set; }
            public string[] Categories { get; set; }
            public string[] Locations { get; set; }
        }

        #endregion

        #region Cleanup

        /// <summary>
        /// Clean up resources
        /// </summary>
        public void Shutdown()
        {
            DisableFileLogging();
            
            lock (_subscriptionsLock)
            {
                _subscriptions.Clear();
            }
        }

        #endregion
    }
}
