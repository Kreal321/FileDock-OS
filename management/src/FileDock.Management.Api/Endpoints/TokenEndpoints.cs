using FileDock.Management.Api.Auth;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Tokens;

namespace FileDock.Management.Api.Endpoints;

public static class TokenEndpoints
{
    public static void MapTokens(this RouteGroupBuilder api)
    {
        var group = api.MapGroup("/tokens").WithMetadata(new AccessScope("S"));
        group.MapGet("", async (ApiTokenService tokens) => Results.Ok((await tokens.ListAsync()).Select(View)));
        group.MapPost("", async (CreateToken body, HttpContext context, ApiTokenService tokens, AuditWriter audit) =>
            await AdminEndpoints.Run(async () =>
            {
                var actor = (Administrator)context.Items["fd.admin"]!;
                var issued = await tokens.CreateAsync(body.Name ?? "", body.Scope ?? "", actor.Id, body.ExpiresAt);
                await AdminEndpoints.WriteAudit(context, audit, "token.create", issued.Record.Id);
                context.Response.Headers.CacheControl = "no-store";
                return Results.Created($"/api/v1/tokens/{issued.Record.Id}", new { issued.Record.Id, issued.Token });
            }));
        group.MapDelete("/{id:guid}", async (Guid id, HttpContext context, ApiTokenService tokens, AuditWriter audit) =>
            await AdminEndpoints.Run(async () =>
            {
                await tokens.RevokeAsync(id);
                await AdminEndpoints.WriteAudit(context, audit, "token.revoke", id);
                return Results.NoContent();
            }));
    }
    private static object View(ApiToken t) => new { t.Id, t.Name, t.Scope, t.CreatedBy, t.CreatedAt, t.ExpiresAt, t.RevokedAt, t.LastUsedAt };
}
public sealed record CreateToken(string? Name, string? Scope, DateTimeOffset? ExpiresAt);
