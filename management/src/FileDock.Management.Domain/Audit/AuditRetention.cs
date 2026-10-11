using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;

namespace FileDock.Management.Domain.Audit;

public sealed partial class AuditRetention(IServiceScopeFactory scopes, ILogger<AuditRetention> logger)
    : BackgroundService
{
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                await using var scope = scopes.CreateAsyncScope();
                var store = scope.ServiceProvider.GetRequiredService<IAuditStore>();
                await store.DeleteOlderThanAsync(DateTimeOffset.UtcNow.AddDays(-90), stoppingToken);
            }
            catch (Exception ex)
            {
                LogRetentionFailed(logger, ex);
            }

            await Task.Delay(TimeSpan.FromHours(24), stoppingToken);
        }
    }

    [LoggerMessage(EventId = 1, Level = LogLevel.Warning, Message = "audit retention failed")]
    private static partial void LogRetentionFailed(ILogger logger, Exception ex);
}
