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
    private static void MapRestore(RouteGroupBuilder api)
    {
        api.MapGet("/restore/report", (AppDbContext db, CoreClient core) => Run(async () =>
        {
            var settings = await db.Settings.SingleAsync(s => s.Id == 1); if (settings.LastRestoreOperationId is null || settings.DismissedRestoreOperationId == settings.LastRestoreOperationId)
            {
                return Results.NoContent();
            }

            return Results.Ok(Json(await core.Recovery.GetRestoreReportAsync(new OperationRef { Id = settings.LastRestoreOperationId }, Headers(false))));
        })).WithMetadata(new AccessScope("R"));
        api.MapPost("/restore/report/dismiss", (AppDbContext db, AuditWriter audit, HttpContext context) => Run(async () => { var settings = await db.Settings.SingleAsync(s => s.Id == 1); settings.DismissedRestoreOperationId = settings.LastRestoreOperationId; await db.SaveChangesAsync(); await Write(audit, context, "restore.report.dismiss", settings.LastRestoreOperationId ?? "appliance", "Succeeded"); return Results.Ok(new { ok = true }); })).WithMetadata(new AccessScope("S"));
        api.MapGet("/setup/restore/local", (CoreClient core, SetupCodeService setup, HttpContext context) => Run(async () => { await Fresh(setup, null, context); return Results.Ok(Json(await core.Recovery.DetectLocalStateAsync(new Empty(), Headers(false)))); })).WithMetadata(new AccessScope("P"));
        api.MapPost("/setup/restore/local", (LocalSource body, CoreClient core, SetupCodeService setup, HttpContext context, RestoreSessions sessions) => Run(async () =>
        {
            await Fresh(setup, null, context); var source = await core.Recovery.OpenLocalRestoreSourceAsync(new OpenLocalRestoreSourceRequest { LocalSourceToken = body.LocalSourceToken, RecoveryKey = body.RecoveryKey }, Headers(true, context)); sessions.Issue(context, source.Id); return Results.Ok(Json(source));
        })).WithMetadata(new AccessScope("P"));
        api.MapPost("/setup/restore/source", (RemoteSource body, CoreClient core, SetupCodeService setup, HttpContext context, RestoreSessions sessions) => Run(async () =>
        {
            await Fresh(setup, body.SetupCode ?? "", context); var request = new OpenRestoreSourceRequest { RecoveryKey = body.RecoveryKey, ProviderFamily = body.ProviderFamily ?? "s3-compatible" };
            if (body.ObjectStorage is { } target)
            {
                request.ObjectStorage = new ObjectStorageTarget { ConnectionId = target.ConnectionId, Bucket = target.Bucket, Prefix = target.Prefix }; request.ConnectionDefinition = Google.Protobuf.WellKnownTypes.Struct.Parser.ParseJson(body.ConnectionDefinition?.GetRawText() ?? "{}"); foreach (var pair in body.Credentials ?? [])
                {
                    request.ConnectionCredentials[pair.Key] = ByteString.CopyFromUtf8(pair.Value);
                }
            }
            else
            {
                request.RemovableDiskPath = body.RemovableDiskPath ?? throw new ArgumentException("Select a source.");
            }

            var source = await core.Recovery.OpenRestoreSourceAsync(request, Headers(true, context)); sessions.Issue(context, source.Id); return Results.Ok(Json(source));
        })).WithMetadata(new AccessScope("P"));
        api.MapPost("/setup/restore/file", (CoreClient core, SetupCodeService setup, HttpContext context, RestoreSessions sessions) => Run(async () =>
        {
            if (!context.Request.HasFormContentType) { throw new ArgumentException("Upload a multipart .fdbackup file."); }
            var form = await context.Request.ReadFormAsync(context.RequestAborted);
            await Fresh(setup, form["setupCode"].ToString(), context);
            var file = form.Files.GetFile("file") ?? throw new ArgumentException("Select a .fdbackup file.");
            if (file.Length is <= 0 or > 128L * 1024 * 1024) { throw new ArgumentException("Backup file exceeds the limit."); }
            using var call = core.Recovery.UploadRestoreFile(Headers(true, context), cancellationToken: context.RequestAborted);
            await using var input = file.OpenReadStream(); var buffer = new byte[65536]; var first = true;
            while (true) { var length = await input.ReadAsync(buffer, context.RequestAborted); if (length == 0) { break; } await call.RequestStream.WriteAsync(new RestoreFileChunk { RecoveryKey = first ? form["recoveryKey"].ToString() : "", Data = ByteString.CopyFrom(buffer, 0, length) }); first = false; }
            await call.RequestStream.CompleteAsync(); var source = await call.ResponseAsync; sessions.Issue(context, source.Id); return Results.Ok(Json(source));
        })).WithMetadata(new AccessScope("P"), new Microsoft.AspNetCore.Mvc.RequestSizeLimitAttribute(140L * 1024 * 1024));
        api.MapGet("/setup/restore/snapshots", (CoreClient core, HttpContext context, RestoreSessions sessions) => Run(async () => Results.Ok(Json(await core.Recovery.ListRestoreSnapshotsAsync(new RestoreSourceRef { Id = sessions.Source(context) }, Headers(false)))))).WithMetadata(new AccessScope("P"));
        api.MapPost("/setup/restore", (Begin body, CoreClient core, HttpContext context, RestoreSessions sessions) => Run(async () =>
        {
            var op = await core.Recovery.BeginRestoreAsync(new BeginRestoreRequest { RestoreSourceId = sessions.Source(context), SnapshotId = body.SnapshotId }, Headers(true, context)); sessions.BindOperation(context, op.Id); return Results.Accepted($"/api/v1/setup/restore/operations/{op.Id}", Json(op));
        })).WithMetadata(new AccessScope("P"));
        api.MapGet("/setup/restore/operations/{id}", (string id, CoreClient core, HttpContext context, RestoreSessions sessions) => Run(async () => { sessions.CheckOperation(context, id); return Results.Ok(Json(await core.Control.GetOperationAsync(new OperationRef { Id = id }, Headers(false)))); })).WithMetadata(new AccessScope("P"));
        api.MapPost("/setup/restore/pool-mappings", (PoolMaps body, CoreClient core, HttpContext context, RestoreSessions sessions) => Run(async () =>
        {
            sessions.CheckOperation(context, body.RestoreOperationId);
            var request = new MapPoolsRequest { RestoreOperationId = body.RestoreOperationId };
            foreach (var mapping in body.Mappings) { var entry = new PoolMapping { PoolId = mapping.PoolId, AdoptExisting = mapping.AdoptExisting, TypedConfirmation = mapping.TypedConfirmation }; entry.DiskSerials.AddRange(mapping.DiskSerials); request.Mappings.Add(entry); }
            return Results.Accepted(value: Json(await core.Recovery.MapPoolsAsync(request, Headers(true, context))));
        })).WithMetadata(new AccessScope("P"));
    }
}
