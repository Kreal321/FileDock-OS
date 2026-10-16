using System.Text.Json;
using Filedock.Core.V1;
using FileDock.Management.Api.Infrastructure;
using FileDock.Management.Core;
using FileDock.Management.Domain.Audit;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
namespace FileDock.Management.Api.Endpoints;

internal static class ResourceEndpoints
{
    private static readonly JsonSerializerOptions Options = new(JsonSerializerDefaults.Web);
    internal static JsonElement Json(IMessage value) => JsonSerializer.Deserialize<JsonElement>(JsonFormatter.Default.Format(value));
    internal static object View(Resource r)
    {
        JsonElement? status = r.Status is null ? null : Json(r.Status);
        if (r.Ref.Kind is ResourceKind.ContainerApp or ResourceKind.Export or ResourceKind.ShareUser && (status is null || !status.Value.TryGetProperty("observedGeneration", out var observed) || !observed.TryGetUInt64(out var generation) || generation != r.Generation)) { status = JsonSerializer.SerializeToElement(new { state = "Pending", detail = "Configuration is awaiting reconciliation." }); }
        return new { id = r.Ref.Id, r.Generation, spec = Json(r.Spec), status };
    }
    internal static async Task<IResult> Page(CoreClient core, ResourceKind kind, HttpContext c)
    {
        var p = PageQuery.From(c.Request); var page = await core.Control.ListResourcesAsync(new ListResourcesRequest { Kind = kind, Page = new PageRequest { PageSize = (uint)Math.Clamp(p.PageSize, 1, 500), PageToken = p.PageToken ?? "" } }, ObservabilityEndpoints.Headers(c, false));
        return Results.Ok(new { items = page.Resources.Select(View), nextPageToken = string.IsNullOrEmpty(page.NextPageToken) ? null : page.NextPageToken });
    }
    internal static async Task<List<Resource>> All(CoreClient core, ResourceKind kind, HttpContext c) { var rows = new List<Resource>(); var cursor = ""; do { var page = await core.Control.ListResourcesAsync(new ListResourcesRequest { Kind = kind, Page = new PageRequest { PageSize = 500, PageToken = cursor } }, ObservabilityEndpoints.Headers(c, false)); rows.AddRange(page.Resources); cursor = page.NextPageToken; } while (cursor.Length > 0); return rows; }
    internal static async Task<Resource> Save(CoreClient core, HttpContext c, ResourceKind kind, string id, object spec)
    {
        ulong generation = 0; if (id.Length > 0) { var match = Concurrency.IfMatch(c.Request); if (match is null || match < 1) { throw new MissingGenerationException(); } generation = (ulong)match; }
        var result = await core.Control.ApplyChangeAsync(new ApplyChangeRequest { Changes = { new ResourceChange { Upsert = new ResourceChange.Types.Upsert { Ref = new ResourceRef { Kind = kind, Id = id }, ExpectedGeneration = generation, Spec = Struct.Parser.ParseJson(JsonSerializer.Serialize(spec, Options)) } } } }, ObservabilityEndpoints.Headers(c, true));
        var row = result.Committed[0]; Concurrency.SetGeneration(c.Response, (long)row.Generation); return row;
    }
    internal static async Task<IResult> Delete(CoreClient core, HttpContext c, AuditWriter audit, ResourceKind kind, string id) { var match = Concurrency.IfMatch(c.Request); if (match is null || match < 1) { return Results.Problem(statusCode: 428, detail: "Supply If-Match with the current generation."); } await core.Control.ApplyChangeAsync(new ApplyChangeRequest { Changes = { new ResourceChange { Delete = new ResourceChange.Types.Delete { Ref = new ResourceRef { Kind = kind, Id = id }, ExpectedGeneration = (ulong)match } } } }, ObservabilityEndpoints.Headers(c, true)); await ObservabilityEndpoints.Audit(c, audit, $"{kind}.deleted", id); return Results.NoContent(); }
}
internal sealed class MissingGenerationException : ArgumentException { public MissingGenerationException() : base("Supply If-Match with the current generation.") { } }
