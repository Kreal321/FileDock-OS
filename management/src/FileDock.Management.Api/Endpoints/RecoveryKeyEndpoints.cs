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
    private static void MapRecoveryKey(RouteGroupBuilder api)
    {
        api.MapPost("/recovery-key/regenerate", (CoreClient core, AuditWriter audit, HttpContext context) => Run(async () =>
        {
            var key = await core.Recovery.GenerateRecoveryKeyAsync(new GenerateRecoveryKeyRequest { Regenerate = true }, Headers(true, context));
            await Write(audit, context, "recovery-key.regenerate", "appliance", "Succeeded");
            return Results.Ok(new { keyId = key.KeyId, recoveryKey = key.RecoveryKey });
        })).WithMetadata(new AccessScope("S"));
        api.MapPost("/recovery-key/confirm", (ConfirmKey body, CoreClient core, HttpContext context) => Run(async () => { await core.Recovery.ConfirmRecoveryKeyAsync(new ConfirmRecoveryKeyRequest { KeyId = body.KeyId }, Headers(true, context)); return Results.Ok(new { ok = true }); })).WithMetadata(new AccessScope("S"));
    }
}
