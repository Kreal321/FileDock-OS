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
    private static readonly JsonSerializerOptions _json = new(JsonSerializerDefaults.Web);
    private static Task Write(AuditWriter audit, HttpContext context, string action, string target, string result)
    {
        var admin = context.Items["fd.admin"] as Administrator; var token = context.Items["fd.token"] as ApiToken;
        return audit.WriteAsync(admin is not null ? "Administrator" : token is not null ? "ApiToken" : "System", admin?.Id.ToString() ?? token?.Id.ToString() ?? "management", admin?.Username ?? token?.Name ?? "management", action, target, result);
    }
    private static Metadata Headers(bool mutate, HttpContext? context = null)
    {
        var admin = context?.Items["fd.admin"] as Administrator; var token = context?.Items["fd.token"] as ApiToken;
        return CoreClient.Metadata(admin is not null ? $"admin:{admin.Username}" : token is not null ? $"token:{token.Id}" : context is not null ? "setup-restore" : "management", Guid.NewGuid().ToString("N"), mutate);
    }
    private static JsonElement Json(IMessage message) => message switch
    {
        Operation operation => JsonSerializer.SerializeToElement(CoreClient.ToOperation(operation), _json),
        StateProtection state => JsonSerializer.SerializeToElement(new { state.RemoteDestinationRegistered, LatestChangeAt = state.LatestChangeAt?.ToDateTimeOffset(), state.ReinstallCovered, state.MachineLossCovered, state.Warning, Copies = state.Copies.Select(c => new { Kind = c.Kind.ToString(), c.PoolId, c.Description, LastSnapshotAt = c.LastSnapshotAt?.ToDateTimeOffset(), c.Stale, c.LastError }) }, _json),
        ListOwnedLocationsResponse locations => JsonSerializer.SerializeToElement(new { Locations = locations.Locations.Select(l => new { l.ConnectionId, l.Bucket, l.Prefix, l.Purpose, l.OwnerApplianceId, l.Generation, LastWriteAt = l.LastWriteAt?.ToDateTimeOffset(), Role = l.Role.ToString(), l.ServedByConnector, l.ServedByThirdParty, ClaimSafety = l.ClaimSafety.ToString() }) }, _json),
        _ => JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(message)),
    };
    private static async Task<IResult> Run(Func<Task<IResult>> action)
    {
        try { return await action(); }
        catch (RpcException ex) { return Results.Problem(statusCode: ex.StatusCode == StatusCode.ResourceExhausted ? 429 : ex.Status.Detail.Contains("backup_file.", StringComparison.Ordinal) || ex.Status.Detail.Contains("claim_ack_required", StringComparison.Ordinal) ? 422 : ex.StatusCode == StatusCode.NotFound ? 404 : ex.StatusCode == StatusCode.Unavailable ? 503 : 409, title: "Recovery request could not finish", detail: ex.Status.Detail); }
        catch (SetupThrottleException ex) { return Results.Problem(statusCode: 429, detail: ex.Message); }
        catch (UnauthorizedAccessException ex) { return Results.Problem(statusCode: 401, detail: ex.Message); }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException or JsonException) { return Results.Problem(statusCode: 400, detail: ex.Message); }
    }
    private static async Task Fresh(SetupCodeService setup, string? code, HttpContext context)
    {
        if (await setup.IsCompleteAsync())
        {
            throw new InvalidOperationException("Restore requires first-run setup.");
        }

        if (code is not null)
        {
            await setup.VerifyAsync(code, context.Connection.RemoteIpAddress?.ToString() ?? "unknown");
        }
    }
    public static RouteGroupBuilder MapRecovery(this RouteGroupBuilder api)
    {
        MapRecoveryKey(api); MapStateProtection(api); MapRestore(api); MapLocation(api); return api;
    }

}
public sealed record ConfirmKey(string KeyId);
public sealed record ObjectDestination(string ConnectionId, string Bucket, string Prefix);
public sealed record Destination(ObjectDestination? ObjectStorage, string? RemovableDiskPath, bool AcknowledgedBasicClaimSafety, bool AcknowledgedThirdPartyConnector);
public sealed record Takeover(string ConnectionId, string Bucket, string Prefix, ulong ObservedGeneration, bool AcknowledgedRecentWriter);
public sealed record LocalSource(string LocalSourceToken, string RecoveryKey);
public sealed record RemoteSource(string? SetupCode, string RecoveryKey, ObjectDestination? ObjectStorage, string? RemovableDiskPath, string? ProviderFamily, JsonElement? ConnectionDefinition, Dictionary<string, string>? Credentials);
public sealed record Begin(string SnapshotId);

public sealed record PoolMaps(string RestoreOperationId, PoolMap[] Mappings);
public sealed record PoolMap(string PoolId, string[] DiskSerials, bool AdoptExisting, string TypedConfirmation);

public sealed record SecretValue(string Value, string[]? Consumers);

public sealed record StateConnection(string Name, string Endpoint, string Region, string Bucket, string AccessKey, string SecretKey);
