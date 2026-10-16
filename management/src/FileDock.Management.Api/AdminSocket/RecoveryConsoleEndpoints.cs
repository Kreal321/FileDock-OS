using System.Text.Json;
using Filedock.Core.V1;
using FileDock.Management.Core;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Setup;
using Google.Protobuf;
using Grpc.Core;
namespace FileDock.Management.Api.AdminSocket;

public static class RecoveryConsoleEndpoints
{
    public static void MapRecoveryConsole(this RouteGroupBuilder admin)
    {
        static Metadata Headers(bool mutate) => CoreClient.Metadata("console:root", Guid.NewGuid().ToString("N"), mutate);
        static JsonElement Json(IMessage message) => JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(message));
        admin.MapPost("/system-pool", async (CoreClient core) => Results.Ok(Json(await core.Storage.GetSystemPoolAsync(new Empty(), Headers(false)))));
        admin.MapPost("/system-pool/limits", async (ConsoleLimits body, CoreClient core, AuditWriter audit) =>
        {
            var old = await core.Storage.GetSystemPoolAsync(new Empty(), Headers(false));
            if (body.ReservedFreeGb > ulong.MaxValue / 1_000_000_000)
            {
                return Results.Problem(statusCode: 422, detail: "system_pool.reserve_invalid");
            }

            var operation = await core.Storage.SetSystemPoolLimitsAsync(new SetSystemPoolLimitsRequest { MaxUsagePercent = body.MaxUsagePercent ?? old.MaxUsagePercent, ReservedFreeBytes = body.ReservedFreeGb is ulong gb ? gb * 1_000_000_000 : old.ReservedFreeBytes, ExpectedGeneration = old.Generation, AcknowledgedInterruption = body.AcknowledgedInterruption }, Headers(true));
            await audit.WriteAsync("Console", "root", "root", "system-pool.limits.changed", old.PoolId, "Succeeded");
            return Results.Accepted($"/api/v1/operations/{operation.Id}", CoreClient.ToOperation(operation));
        });
        admin.MapPost("/state-protection", async (CoreClient core) => Results.Ok(Json(await core.Recovery.GetStateProtectionAsync(new Empty(), Headers(false)))));
        admin.MapPost("/detect-state", async (CoreClient core) => Results.Ok(Json(await core.Recovery.DetectLocalStateAsync(new Empty(), Headers(false)))));
        admin.MapPost("/recovery-key/regenerate", async (CoreClient core, AuditWriter audit) =>
        {
            var key = await core.Recovery.GenerateRecoveryKeyAsync(new GenerateRecoveryKeyRequest { Regenerate = true }, Headers(true));
            await audit.WriteAsync("Console", "root", "root", "recovery-key.regenerate", "appliance", "Succeeded");
            return Results.Ok(new { keyId = key.KeyId, recoveryKey = key.RecoveryKey });
        });
        admin.MapPost("/recovery-key/confirm", async (ConfirmKey body, CoreClient core) => { await core.Recovery.ConfirmRecoveryKeyAsync(new ConfirmRecoveryKeyRequest { KeyId = body.KeyId }, Headers(true)); return Results.Ok(new { ok = true }); });
        admin.MapPost("/backup-file", async (CoreClient core, AuditWriter audit) =>
        {
            using var call = core.Recovery.ExportBackupFile(new Empty(), Headers(false)); using var bytes = new MemoryStream();
            await foreach (var chunk in call.ResponseStream.ReadAllAsync()) { if (bytes.Length + chunk.Data.Length > 128L * 1024 * 1024) { throw new InvalidOperationException("Backup too large."); } chunk.Data.WriteTo(bytes); }
            await audit.WriteAsync("Console", "root", "root", "appliance.backup-export", "appliance", "Succeeded"); return Results.File(bytes.ToArray(), "application/octet-stream", "filedock.fdbackup");
        });
        admin.MapPost("/restore-operation", async (OperationRef body, CoreClient core) => Results.Ok(CoreClient.ToOperation(await core.Control.GetOperationAsync(body, Headers(false)))));
        admin.MapPost("/restore-local", async (RestoreLocal body, CoreClient core, SetupCodeService setup) =>
        {
            if (await setup.IsCompleteAsync()) { return Results.Problem(statusCode: 409, detail: "Restore requires a fresh appliance."); }
            var source = await core.Recovery.OpenLocalRestoreSourceAsync(new OpenLocalRestoreSourceRequest { LocalSourceToken = body.LocalSourceToken, RecoveryKey = body.RecoveryKey }, Headers(true));
            var snapshots = await core.Recovery.ListRestoreSnapshotsAsync(new RestoreSourceRef { Id = source.Id }, Headers(false));
            var selected = snapshots.Snapshots.FirstOrDefault(s => s.NewestValid) ?? throw new InvalidOperationException("No valid snapshot.");
            var operation = await core.Recovery.BeginRestoreAsync(new BeginRestoreRequest { RestoreSourceId = source.Id, SnapshotId = selected.SnapshotId }, Headers(true));
            return Results.Accepted(value: Json(operation));
        });
    }
}
public sealed record RestoreLocal(string LocalSourceToken, string RecoveryKey);
public sealed record ConfirmKey(string KeyId);

public sealed record ConsoleLimits(uint? MaxUsagePercent, ulong? ReservedFreeGb, bool AcknowledgedInterruption = false);
