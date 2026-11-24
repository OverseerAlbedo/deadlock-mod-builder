using System.Collections.Concurrent;
using Microsoft.Extensions.Logging;

namespace Dmb.EffectPlayer;

/// <summary>
/// Collects every message the libraries log, so a run can report them. Each message gets a
/// sequence number, so a caller can take the messages logged since a mark. At most
/// <see cref="Capacity"/> messages are kept; older ones are dropped. With <c>echo</c> on, messages
/// at or above <c>echoLevel</c> are also written to stderr, one plain line each.
/// </summary>
sealed class CollectingLogger(bool echo, LogLevel echoLevel = LogLevel.Debug) : ILogger
{
    const int Capacity = 4000;

    long sequence;

    public ConcurrentQueue<(LogLevel Level, string Message)> Messages { get; } = new();

    readonly ConcurrentQueue<(long Sequence, LogLevel Level, string Message)> numbered = new();

    /// <summary>The sequence number the next message will get.</summary>
    public long Mark => Interlocked.Read(ref sequence);

    public IDisposable? BeginScope<TState>(TState state) where TState : notnull => null;

    public bool IsEnabled(LogLevel logLevel) => logLevel >= LogLevel.Debug;

    public void Log<TState>(LogLevel logLevel, EventId eventId, TState state, Exception? exception, Func<TState, Exception?, string> formatter)
    {
        var text = formatter(state, exception);
        if (exception != null)
        {
            text += $" ({exception.GetType().Name}: {exception.Message})";
        }

        text = text.Replace('\r', ' ').Replace('\n', ' ');
        var number = Interlocked.Increment(ref sequence) - 1;

        Messages.Enqueue((logLevel, text));
        numbered.Enqueue((number, logLevel, text));
        while (Messages.Count > Capacity && Messages.TryDequeue(out _))
        {
        }

        while (numbered.Count > Capacity && numbered.TryDequeue(out _))
        {
        }

        if (echo && logLevel >= echoLevel)
        {
            Console.Error.WriteLine($"[{logLevel}] {text}");
        }
    }

    /// <summary>The messages logged since <paramref name="mark"/> that are still kept.</summary>
    public List<(LogLevel Level, string Message)> Since(long mark)
        => [.. numbered.Where(m => m.Sequence >= mark).Select(m => (m.Level, m.Message))];
}
