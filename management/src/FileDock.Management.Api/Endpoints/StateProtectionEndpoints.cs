using System.Text.Json;
using Filedock.Core.V1;
using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using FileDock.Management.Data;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Setup;
using Google.Protobuf;
using Grpc.Core;
using Microsoft.EntityFrameworkCore;
namespace FileDock.Management.Api.Endpoints;

public static partial class RecoveryEndpoints
{
    private static void MapStateProtection(RouteGroupBuilder api)
    {
        api.MapGet("/state-protection", (CoreClient core) => Run(async () => Results.Ok(Json(await core.Recovery.GetStateProtectionAsync(new Empty(), Headers(false)))))).WithMetadata(new AccessScope("R"));
        api.MapPut("/state-protection/destination", (Destination body, CoreClient core, AuditWriter audit, HttpContext context) => Run(async () =>
        {
            var request = new SetStateDestinationRequest { AcknowledgedBasicClaimSafety = body.AcknowledgedBasicClaimSafety, AcknowledgedThirdPartyConnector = body.AcknowledgedThirdPartyConnector };
            if (body.ObjectStorage is { } target)
            {
                request.ObjectStorage = new ObjectStorageTarget { ConnectionId = target.ConnectionId, Bucket = target.Bucket, Prefix = target.Prefix };
            }
            else
            {
                request.RemovableDiskPath = body.RemovableDiskPath ?? throw new ArgumentException("Select a destination.");
            }

            var op = await core.Recovery.SetStateDestinationAsync(request, Headers(true, context));
            await Write(audit, context, "state.destination", "appliance", "Accepted");
            return Results.Accepted($"/api/v1/operations/{op.Id}", Json(op));
        })).WithMetadata(new AccessScope("S"));
        api.MapPost("/state-protection/snapshot", (CoreClient core, HttpContext context) => Run(async () =>
        {
            var op = await core.Control.StartOperationAsync(new StartOperationRequest { Kind = "StateSnapshot" }, Headers(true, context)); return Results.Accepted($"/api/v1/operations/{op.Id}", Json(op));
        })).WithMetadata(new AccessScope("F"));
        api.MapGet("/state-protection/backup-file", (CoreClient core, AuditWriter audit, HttpContext context) => Run(async () =>
        {
            using var call = core.Recovery.ExportBackupFile(new Empty(), Headers(false)); using var bytes = new MemoryStream();
            await foreach (var chunk in call.ResponseStream.ReadAllAsync()) { if (bytes.Length + chunk.Data.Length > 128L * 1024 * 1024) { throw new InvalidOperationException("Backup file exceeds the limit."); } chunk.Data.WriteTo(bytes); }
            await Write(audit, context, "appliance.backup_exported", "appliance", "Succeeded");
            return Results.File(bytes.ToArray(), "application/octet-stream", $"filedock-{DateTime.UtcNow:yyyyMMddTHHmmssZ}.fdbackup");
        })).WithMetadata(new AccessScope("S"));
        api.MapPost("/state-protection/connections", (StateConnection body, CoreClient core, AuditWriter audit, HttpContext context) => Run(async () =>
        {
            var id = Guid.NewGuid().ToString(); var access = $"connection/{id}/access-key"; var secret = $"connection/{id}/secret-key";
            var spec = Google.Protobuf.WellKnownTypes.Struct.Parser.ParseJson(JsonSerializer.Serialize(new { name = body.Name, providerFamily = "s3-compatible", definition = new { type = "s3", endpoint = body.Endpoint, region = body.Region, addressingStyle = "Path", bucket = body.Bucket, accessKeyRef = access, secretKeyRef = secret } }));
            var change = new ResourceChange { Upsert = new ResourceChange.Types.Upsert { Ref = new ResourceRef { Kind = ResourceKind.CloudConnection, Id = id }, ExpectedGeneration = 0, Spec = spec } };
            var validation = await core.Control.ValidateChangeAsync(new ApplyChangeRequest { Changes = { change } }, Headers(true, context)); if (validation.Issues.Any(i => i.Severity == ValidationIssue.Types.Severity.Error)) { throw new ArgumentException("Invalid connection definition. Use a valid HTTPS endpoint and bucket."); }
            foreach (var pair in new[] { (access, body.AccessKey), (secret, body.SecretKey) }) { await core.Secrets.PutSecretAsync(new PutSecretRequest { Ref = pair.Item1, Value = ByteString.CopyFromUtf8(pair.Item2), Consumers = { "fd-s3" } }, Headers(true, context)); }
            await core.Control.ApplyChangeAsync(new ApplyChangeRequest { Changes = { change } }, Headers(true, context));
            var probe = await core.Connectors.TestConnectionAsync(new ConnectionRef { ConnectionId = id }, Headers(true, context));
            await Write(audit, context, "state.connection.created", id, "Succeeded"); return Results.Created($"/api/v1/state-protection/connections/{id}", new { connectionId = id, probe = Json(probe) });
        })).WithMetadata(new AccessScope("S"));
        api.MapGet("/secrets", (CoreClient core) => Run(async () => Results.Ok(Json(await core.Secrets.ListSecretsAsync(new ListSecretsRequest(), Headers(false)))))).WithMetadata(new AccessScope("R"));
        api.MapPut("/secrets/{**reference}", (string reference, SecretValue body, CoreClient core, AuditWriter audit, HttpContext context) => Run(async () =>
        {
            await core.Secrets.PutSecretAsync(new PutSecretRequest { Ref = reference, Value = ByteString.CopyFromUtf8(body.Value), Consumers = { body.Consumers ?? [] } }, Headers(true, context));
            await Write(audit, context, "secret.updated", reference, "Succeeded"); return Results.Ok(new { ok = true });
        })).WithMetadata(new AccessScope("S"));
    }
}
