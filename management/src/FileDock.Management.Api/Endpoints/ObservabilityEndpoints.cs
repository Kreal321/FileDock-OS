using Empty = Filedock.Core.V1.Empty;
using System.Globalization;
using System.Text.Json;
using Filedock.Core.V1;
using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Audit;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using Grpc.Core;
namespace FileDock.Management.Api.Endpoints;

public static class ObservabilityEndpoints
{
    public static Metadata Headers(HttpContext c, bool mutate) => CoreClient.Metadata(c.User.Identity?.Name ?? "management", c.TraceIdentifier, mutate);
    public static Task Audit(HttpContext c, AuditWriter writer, string action, string target, object? detail = null) => writer.WriteAsync(c.Items["fd.token"] is ApiToken ? "ApiToken" : "Administrator", c.User.FindFirst(System.Security.Claims.ClaimTypes.NameIdentifier)?.Value ?? "management", c.User.Identity?.Name ?? "management", action, target, "Succeeded", detail);
    public static async Task<IResult> Run(Func<Task<IResult>> action)
    {
        try { return await action(); }
        catch (MissingGenerationException e) { return Results.Problem(statusCode: 428, detail: e.Message); }
        catch (RpcException e) { var (status, detail) = CoreClient.ToProblem(e); return Results.Problem(statusCode: status == 412 ? 422 : status, detail: detail); }
        catch (Exception e) when (e is ArgumentException or FormatException or JsonException) { return Results.Problem(statusCode: 400, detail: e.Message); }
    }
    private static PageRequest Page(HttpContext c) => new() { PageSize = uint.TryParse(c.Request.Query["pageSize"], out var n) ? Math.Clamp(n, 1u, 500u) : 100, PageToken = c.Request.Query["pageToken"].ToString() };
    public static object Channel(NotificationChannel c) => new { c.Id, c.SubsystemId, c.Name, c.MinSeverity, Config = JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(c.Config ?? new Struct())), c.SecretRefs };
    private static NotificationChannel ChannelBody(ChannelInput b, string id) => new() { Id = id, Name = b.Name, SubsystemId = b.SubsystemId, MinSeverity = b.MinSeverity, Config = Struct.Parser.ParseJson(b.Config.ValueKind == JsonValueKind.Undefined ? "{}" : b.Config.GetRawText()), SecretRefs = { b.SecretRefs ?? [] } };
    public static void MapObservability(this RouteGroupBuilder api)
    {
        api.MapGet("/logs", (HttpContext c, CoreClient core) => Run(async () =>
        {
            Timestamp? Time(string key) => string.IsNullOrEmpty(c.Request.Query[key]) ? null : Timestamp.FromDateTimeOffset(DateTimeOffset.Parse(c.Request.Query[key]!, CultureInfo.InvariantCulture));
            var r = new QueryLogsRequest { Page = Page(c), MinSeverity = c.Request.Query["minSeverity"].ToString(), Text = c.Request.Query["text"].ToString(), Since = Time("since"), Until = Time("until") }; r.Sources.AddRange(c.Request.Query["source"].SelectMany(v => (v ?? "").Split(',', StringSplitOptions.RemoveEmptyEntries)));
            var page = await core.Observability.QueryLogsAsync(r, Headers(c, false));
            return Results.Ok(new { Items = page.Entries.Select(e => new { At = e.At?.ToDateTimeOffset(), e.Source, e.Severity, e.Message }), NextPageToken = string.IsNullOrEmpty(page.NextPageToken) ? null : page.NextPageToken });
        })).WithMetadata(new AccessScope("R"));
        api.MapGet("/activity", (HttpContext c, CoreClient core) => Run(async () =>
        {
            var page = await core.Observability.QueryActivityAsync(Page(c), Headers(c, false)); return Results.Ok(new { Items = page.Events.Select(e => new { e.Seq, At = e.At?.ToDateTimeOffset(), Type = e.Type, Subject = e.Subject, Payload = e.Payload is null ? (JsonElement?)null : JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(e.Payload)) }), NextPageToken = string.IsNullOrEmpty(page.NextPageToken) ? null : page.NextPageToken });
        })).WithMetadata(new AccessScope("R"));
        api.MapGet("/diagnostics/bundle", (HttpContext c, CoreClient core, AuditWriter audit) => Run(async () => { var bytes = await Bundle(core, Headers(c, true), c.RequestAborted); await Audit(c, audit, "diagnostics.exported", "appliance"); return Results.File(bytes, "application/zstd", "filedock-diagnostics.tar.zst"); })).WithMetadata(new AccessScope("F"));
        api.MapGet("/alerts", (HttpContext c, CoreClient core) => Run(async () =>
        {
            var r = new ListAlertsRequest { Page = Page(c), MinSeverity = c.Request.Query["minSeverity"].ToString() }; r.States.AddRange(c.Request.Query["state"].SelectMany(v => (v ?? "").Split(',', StringSplitOptions.RemoveEmptyEntries)));
            var page = await core.Observability.ListAlertsAsync(r, Headers(c, false)); return Results.Ok(new { Items = page.Alerts.Select(a => new { a.Id, a.Severity, a.Kind, a.Source, a.Message, a.State, FirstSeenAt = a.FirstSeenAt?.ToDateTimeOffset(), LastSeenAt = a.LastSeenAt?.ToDateTimeOffset(), a.AcknowledgedBy }), NextPageToken = string.IsNullOrEmpty(page.NextPageToken) ? null : page.NextPageToken });
        })).WithMetadata(new AccessScope("R"));
        api.MapPost("/alerts/{id}/ack", (string id, HttpContext c, CoreClient core, AuditWriter audit) => Run(async () => { await core.Observability.AcknowledgeAlertAsync(new AlertRef { Id = id }, Headers(c, true)); await Audit(c, audit, "alert.acknowledged", id); return Results.NoContent(); })).WithMetadata(new AccessScope("F"));
        api.MapGet("/notification-channels", (HttpContext c, CoreClient core) => Run(async () => Results.Ok(new { Items = (await core.Extensions.ListNotificationChannelsAsync(new Empty(), Headers(c, false))).Channels.Select(Channel) }))).WithMetadata(new AccessScope("R"));
        api.MapPost("/notification-channels", (ChannelInput b, HttpContext c, CoreClient core, AuditWriter audit) => Save(b, "", c, core, audit)).WithMetadata(new AccessScope("F"));
        api.MapPatch("/notification-channels/{id}", (string id, ChannelInput b, HttpContext c, CoreClient core, AuditWriter audit) => Save(b, id, c, core, audit)).WithMetadata(new AccessScope("F"));
        api.MapDelete("/notification-channels/{id}", (string id, HttpContext c, CoreClient core, AuditWriter audit) => Run(async () => { await core.Extensions.RemoveNotificationChannelAsync(new NotificationChannelRef { Id = id }, Headers(c, true)); await Audit(c, audit, "notification.removed", id); return Results.NoContent(); })).WithMetadata(new AccessScope("F"));
        api.MapPost("/notification-channels/{id}/test", (string id, HttpContext c, CoreClient core, AuditWriter audit) => Run(async () => { await core.Extensions.TestNotificationChannelAsync(new NotificationChannelRef { Id = id }, Headers(c, true)); await Audit(c, audit, "notification.test", id); return Results.Ok(new { delivered = true }); })).WithMetadata(new AccessScope("F"));
    }
    private static Task<IResult> Save(ChannelInput b, string id, HttpContext c, CoreClient core, AuditWriter audit) => Run(async () => { var saved = await core.Extensions.ConfigureNotificationChannelAsync(ChannelBody(b, id), Headers(c, true)); await Audit(c, audit, "notification.configured", saved.Id); return Results.Ok(Channel(saved)); });
    public static async Task<byte[]> Bundle(CoreClient core, Metadata headers, CancellationToken ct)
    {
        using var stream = core.Observability.CreateDiagnosticBundle(new Empty(), headers, cancellationToken: ct); using var data = new MemoryStream();
        while (await stream.ResponseStream.MoveNext(ct)) { if (data.Length + stream.ResponseStream.Current.Data.Length > 64 * 1024 * 1024) { throw new IOException("Diagnostic bundle exceeded the download limit."); } await data.WriteAsync(stream.ResponseStream.Current.Data.Memory, ct); }
        return data.ToArray();
    }
}
public sealed record ChannelInput(string Name, string SubsystemId, string MinSeverity, JsonElement Config, string[]? SecretRefs);
