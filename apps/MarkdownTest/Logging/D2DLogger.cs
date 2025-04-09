using System;
using System.Collections.Generic;
using BlitzWinRT;

namespace MarkdownTest.Logging
{
    /// <summary>
    /// WinRT-compatible logger that implements the BlitzWinRT.ILogger interface
    /// and uses the LogManager for actual logging
    /// </summary>
    public class D2DLogger : ILogger
    {
        private readonly string _defaultCategory;
        private readonly string _defaultLocation;
        private int _messageCounter = 0;
        
        /// <summary>
        /// Creates a new D2DLogger with the specified default settings
        /// </summary>
        public D2DLogger(string defaultLocation = "Unknown", string defaultCategory = "Unknown")
        {
            _defaultLocation = defaultLocation;
            _defaultCategory = defaultCategory;
            
            // Log creation of logger instance
            LogManager.Instance.Log($"D2DLogger created with default location '{defaultLocation}' and category '{defaultCategory}'", 
                category: "Logger", location: defaultLocation);
        }
        
        /// <summary>
        /// Legacy method for backward compatibility
        /// </summary>
        public void LogMessage(string message)
        {
            // For backward compatibility, route the message through the new system
            LogManager.Instance.Log(message, _defaultCategory, _defaultLocation);
        }
        
        /// <summary>
        /// Logs a message with the specified category and location
        /// </summary>
        public void LogWithCategory(string message, string category, string location)
        {
            LogManager.Instance.Log(message, category, location);
        }
        
        /// <summary>
        /// Get all logs formatted as a string
        /// </summary>
        public static string GetAllLogs()
        {
            var logs = LogManager.Instance.GetLogs();
            return string.Join(Environment.NewLine, logs);
        }
        
        /// <summary>
        /// Clear all in-memory logs
        /// </summary>
        public static void ClearLogs()
        {
            LogManager.Instance.ClearLogs();
        }
        
        /// <summary>
        /// Enable logging to a file
        /// </summary>
        public static void EnableFileLogging(string filePath)
        {
            LogManager.Instance.EnableFileLogging(filePath);
        }
        
        /// <summary>
        /// Disable logging to a file
        /// </summary>
        public static void DisableFileLogging()
        {
            LogManager.Instance.DisableFileLogging();
        }
        
        /// <summary>
        /// Export logs to a file
        /// </summary>
        public static void ExportLogs(string filePath)
        {
            LogManager.Instance.ExportLogsToFileAsync(filePath).ConfigureAwait(false);
        }
    }
}
