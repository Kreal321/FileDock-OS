using System.Text.Json;
using FileDock.Management.Core;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Setup;
using Filedock.Core.V1;
using Google.Protobuf;
using Grpc.Core;
namespace FileDock.Management.Api.AdminSocket;

public static class RecoveryConsoleEndpoints
{
    public static void MapRecoveryConsole(this RouteGroupBuilder admin)
    {
        static Metadata Headers(bool mutate) => CoreClient.Metadata("console:root", Guid.NewGuid().ToString("N"), mutate);
        static JsonElement Json(IMessage message) => JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(message));
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
