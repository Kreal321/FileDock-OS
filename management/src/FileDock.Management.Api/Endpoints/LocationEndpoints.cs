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
    private static void MapLocation(RouteGroupBuilder api)
    {
        api.MapGet("/locations", (CoreClient core) => Run(async () => Results.Ok(Json(await core.Recovery.ListOwnedLocationsAsync(new Empty(), Headers(false)))))).WithMetadata(new AccessScope("R"));
        api.MapPost("/locations/takeover", (Takeover body, CoreClient core, AuditWriter audit, HttpContext context) => Run(async () =>
        {
            var op = await core.Recovery.ConfirmTakeoverAsync(new ConfirmTakeoverRequest { ConnectionId = body.ConnectionId, Bucket = body.Bucket, Prefix = body.Prefix, ObservedGeneration = body.ObservedGeneration, AcknowledgedRecentWriter = body.AcknowledgedRecentWriter }, Headers(true, context));
            await Write(audit, context, "location.takeover", body.ConnectionId, "Succeeded"); return Results.Accepted($"/api/v1/operations/{op.Id}", Json(op));
        })).WithMetadata(new AccessScope("S"));
    }
}
