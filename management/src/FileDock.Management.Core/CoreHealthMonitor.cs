using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

namespace FileDock.Management.Core;

public sealed record CoreHealth(string State, string Detail, string? InterfaceVersion)
{
    public bool CanMutate => State == "Healthy";
}

public sealed class CoreHealthMonitor(ICoreGateway gateway, IManagementEventSink sink, ILogger<CoreHealthMonitor> logger) : BackgroundService
{
    private CoreHealth _current = new("Starting", "Checking the Core connection…", null);
    public CoreHealth Current => Volatile.Read(ref _current);
    public SystemStatusDto? LastStatus { get; private set; }
    private int _streamConnected;
    public void ReportUnavailable() => Interlocked.Exchange(ref _current, new("Unreachable", "Core is unreachable. Changes are disabled until it returns.", Current.InterfaceVersion));
    public void StreamConnected() => Interlocked.Exchange(ref _streamConnected, 1);
    public void StreamLost() { Interlocked.Exchange(ref _streamConnected, 0); Interlocked.Exchange(ref _current, new("Unreachable", "Core event stream disconnected. Changes are disabled until it reconnects.", Current.InterfaceVersion)); }
    public static bool IsCompatible(string version) => Version.TryParse(version, out var peer)
        && Version.TryParse(CoreClient.InterfaceVersion, out var ours) && peer.Major == ours.Major;
    private static readonly Action<ILogger, string, Exception?> _probeFailed = LoggerMessage.Define<string>(LogLevel.Debug, new EventId(101, "ProbeFailed"), "Core health probe failed ({ErrorType})");
    private static readonly Action<ILogger, string, Exception?> _publishFailed = LoggerMessage.Define<string>(LogLevel.Warning, new EventId(102, "PublishFailed"), "Could not relay Core health ({ErrorType})");
    private static void LogProbeFailed(ILogger logger, string type) => _probeFailed(logger, type, null);
    private static void LogPublishFailed(ILogger logger, string type) => _publishFailed(logger, type, null);
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        CoreHealth? published = null;
        while (!stoppingToken.IsCancellationRequested)
        {
            CoreHealth status;
            try
            {
                using var timeout = CancellationTokenSource.CreateLinkedTokenSource(stoppingToken);
                timeout.CancelAfter(TimeSpan.FromSeconds(2));
                var core = await gateway.GetSystemStatusAsync(timeout.Token);
                LastStatus = core;
                status = IsCompatible(core.InterfaceVersion)
                    ? Volatile.Read(ref _streamConnected) == 1
                        ? core.Components.Any(c => c.Id == "core" && c.State != "Healthy")
                            ? new("Starting", "Core startup reconciliation is pending.", core.InterfaceVersion)
                            : new("Healthy", "Core connected", core.InterfaceVersion)
                        : new("Unreachable", "Core event stream disconnected. Changes are disabled until it reconnects.", core.InterfaceVersion)
                    : new("Incompatible", $"Core interface {core.InterfaceVersion} is incompatible with Management {CoreClient.InterfaceVersion}. Changes are disabled.", core.InterfaceVersion);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { break; }
            catch (Exception error)
            {
                if (logger.IsEnabled(LogLevel.Debug)) { LogProbeFailed(logger, error.GetType().Name); }
                status = new("Unreachable", "Core is unreachable. Changes are disabled until it returns.", Current.InterfaceVersion);
            }
            Interlocked.Exchange(ref _current, status);
            if (status != published)
            {
                try { await sink.PublishHealthAsync(status, stoppingToken); published = status; }
                catch (Exception error) when (error is not OperationCanceledException) { LogPublishFailed(logger, error.GetType().Name); }
            }
            await Task.Delay(TimeSpan.FromSeconds(1), stoppingToken);
        }
    }
}
