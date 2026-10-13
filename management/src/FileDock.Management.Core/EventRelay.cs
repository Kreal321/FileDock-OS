using System.Text.Json;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

namespace FileDock.Management.Core;

public sealed record CoreEventDto(ulong Seq, DateTimeOffset? At, string Type, string? SubjectKind, string? SubjectId, JsonElement Payload);
public interface ICoreEventSource
{
    IAsyncEnumerable<CoreEventDto> StreamEventsAsync(ulong afterSeq, Action connected, CancellationToken cancellationToken);
}
public interface IManagementEventSink
{
    Task<ulong> ReadCursorAsync(CancellationToken cancellationToken);
    Task PublishAsync(CoreEventDto change, CancellationToken cancellationToken);
    Task SaveCursorAsync(ulong seq, CancellationToken cancellationToken);
    Task PublishHealthAsync(CoreHealth health, CancellationToken cancellationToken);
}

public sealed class EventRelay(ICoreEventSource source, IManagementEventSink sink, CoreHealthMonitor health, ILogger<EventRelay> logger) : BackgroundService
{
    private static readonly Action<ILogger, string, Exception?> _disconnected = LoggerMessage.Define<string>(LogLevel.Warning, new EventId(100, "StreamDisconnected"), "Core event stream disconnected ({ErrorType}); retrying");
    private static void LogDisconnected(ILogger logger, string type) => _disconnected(logger, type, null);
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        ulong cursor = 0;
        var loaded = false;
        var delay = 1;
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                if (!loaded) { cursor = await sink.ReadCursorAsync(stoppingToken); loaded = true; }
                await foreach (var change in source.StreamEventsAsync(cursor, health.StreamConnected, stoppingToken))
                {
                    if (change.Seq <= cursor) { continue; }
                    // Publish before persisting; a crash can duplicate an event but cannot lose it.
                    await sink.PublishAsync(change, stoppingToken);
                    await sink.SaveCursorAsync(change.Seq, stoppingToken);
                    cursor = change.Seq;
                    delay = 1;
                }
                health.StreamLost();
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { break; }
            catch (Exception error)
            {
                health.StreamLost();
                LogDisconnected(logger, error.GetType().Name);
            }
            await Task.Delay(TimeSpan.FromSeconds(delay), stoppingToken);
            delay = Math.Min(delay * 2, 30);
        }
    }
}
