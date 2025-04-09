using System;

namespace MarkdownTest.Logging
{
    /// <summary>
    /// Represents a single log entry with metadata
    /// </summary>
    public class LogEntry
    {
        /// <summary>
        /// Unique identifier for this log entry
        /// </summary>
        public int Id { get; }

        /// <summary>
        /// The message content of the log
        /// </summary>
        public string Message { get; }

        /// <summary>
        /// The category of the log (e.g., "Error", "Warning", "Info")
        /// </summary>
        public string Category { get; }

        /// <summary>
        /// The location that generated the log (e.g., "Rust", "C#", "UI")
        /// </summary>
        public string Location { get; }

        /// <summary>
        /// The timestamp when the log was created
        /// </summary>
        public DateTime Timestamp { get; }

        /// <summary>
        /// Creates a new log entry
        /// </summary>
        public LogEntry(int id, string message, string category, string location, DateTime timestamp)
        {
            Id = id;
            Message = message ?? string.Empty;
            Category = category ?? "Unknown";
            Location = location ?? "Unknown";
            Timestamp = timestamp;
        }

        /// <summary>
        /// Returns a formatted string representation of the log entry
        /// </summary>
        public override string ToString()
        {
            return $"[{Id}] {Timestamp:HH:mm:ss.fff} [{Category}] [{Location}] - {Message}";
        }
    }
}
