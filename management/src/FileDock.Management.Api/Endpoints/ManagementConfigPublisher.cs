using Filedock.Core.V1;
using FileDock.Management.Core;
using FileDock.Management.Domain.Restore;
using Google.Protobuf;
using Grpc.Core;
namespace FileDock.Management.Api.Endpoints;
/// Publish durable desired configuration; import verified recovery documents before publishing.
public sealed partial class ManagementConfigPublisher(IServiceScopeFactory scopes, CoreClient core, ILogger<ManagementConfigPublisher> log) : BackgroundService
{
    [LoggerMessage(EventId = 6001, Level = LogLevel.Warning, Message = "Management configuration publish/import pending ({ErrorType}).")]
    private static partial void Pending(ILogger logger, string errorType);
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        byte[]? published = null;
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                using var scope = scopes.CreateScope();
                var store = scope.ServiceProvider.GetRequiredService<IManagementConfigStore>();
                var operations = await core.Control.ListOperationsAsync(new ListOperationsRequest { ActiveOnly = true, Page = new PageRequest { PageSize = 100 } }, headers: CoreClient.Metadata("management", "restore-resume", false), cancellationToken: stoppingToken);
                foreach (var operation in operations.Operations.Where(o => o.Kind == "Restore" && o.Steps.Any(s => s.Name == "import-core" && s.State == "done") && o.Steps.Any(s => s.Name == "import-management" && s.State != "done")))
                {
                    var document = await core.Recovery.GetManagementConfigForRestoreAsync(new RestoreSourceRef { Id = operation.Id }, headers: CoreClient.Metadata("management", "restore-import", false), cancellationToken: stoppingToken);
                    await store.ImportAsync(document.Document.ToByteArray(), operation.Id, stoppingToken);
                    var network = scope.ServiceProvider.GetRequiredService<FileDock.Management.Api.Network.NetworkService>(); network.Apply(await network.GetBindableAsync());
                    published = null;
                }
                var db = scope.ServiceProvider.GetRequiredService<FileDock.Management.Data.AppDbContext>();
                if (await Microsoft.EntityFrameworkCore.EntityFrameworkQueryableExtensions.AnyAsync(db.Administrators, stoppingToken))
                {
                    var completed = await core.Control.ListOperationsAsync(new ListOperationsRequest { Page = new PageRequest { PageSize = 100 } }, headers: CoreClient.Metadata("management", "takeover-audit", false), cancellationToken: stoppingToken);
                    foreach (var operation in completed.Operations.Where(o => o.Kind == "LocationTakeover" && o.StartedBy == "system" && o.State == Operation.Types.State.Succeeded))
                    {
                        if (!await Microsoft.EntityFrameworkCore.EntityFrameworkQueryableExtensions.AnyAsync(db.AuditEntries, a => a.Action == "location.takeover" && a.Target == operation.Id, stoppingToken))
                        {
                            db.AuditEntries.Add(new FileDock.Management.Domain.AuditEntry { Id = Guid.NewGuid(), At = DateTimeOffset.UtcNow, ActorKind = "System", ActorId = "system", ActorName = "system", Action = "location.takeover", Target = operation.Id, Result = "Succeeded", Detail = "{\"reason\":\"reinstall-restore\"}" }); await db.SaveChangesAsync(stoppingToken);
                        }
                    }
                }
                var bytes = await store.ExportAsync(stoppingToken);
                if (published is null || !bytes.AsSpan().SequenceEqual(published))
                {
                    var config = System.Text.Json.JsonSerializer.Deserialize<ManagementConfiguration>(bytes, ManagementRestoreService.Json)!;
                    if (config.Administrators.Length > 0)
                    {
                        await core.Recovery.PutManagementConfigAsync(new ManagementConfig { SchemaVersion = "fdconfig.management/1", Document = ByteString.CopyFrom(bytes), ConfigSeq = checked((ulong)config.ConfigSeq), ExportedAt = Google.Protobuf.WellKnownTypes.Timestamp.FromDateTimeOffset(config.ConfigChangedAt ?? DateTimeOffset.UtcNow) }, headers: CoreClient.Metadata("management", "config-publish", true), cancellationToken: stoppingToken);
                        published = bytes;
                    }
                }
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested) { break; }
            catch (Exception ex) when (ex is RpcException or InvalidOperationException or ArgumentException)
            { Pending(log, ex.GetType().Name); }
            await Task.Delay(TimeSpan.FromSeconds(2), stoppingToken);
        }
    }
}
