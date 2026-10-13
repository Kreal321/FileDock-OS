using FileDock.Management.Api.Auth;
using FileDock.Management.Api.Infrastructure;
using FileDock.Management.Data;
using FileDock.Management.Domain;
using Microsoft.EntityFrameworkCore;

namespace FileDock.Management.Api.Endpoints;

public static class AuditEndpoints
{
    public static void MapAudit(this RouteGroupBuilder api)
    {
        api.MapGet("/audit", async (HttpContext context, AppDbContext db) =>
        {
            var page = PageQuery.From(context.Request);
            var offset = 0;
            if (page.PageToken is not null && (!int.TryParse(page.PageToken, out offset) || offset < 0))
            {
                return AdminEndpoints.Problem(400, "Invalid page token.");
            }
            var query = db.AuditEntries.AsNoTracking().AsQueryable();
            var actor = context.Request.Query["actor"].ToString();
            var action = context.Request.Query["action"].ToString();
            if (!string.IsNullOrEmpty(actor)) { query = query.Where(e => e.ActorName == actor); }
            if (!string.IsNullOrEmpty(action)) { query = query.Where(e => e.Action == action); }
            // UUIDv7 identifiers preserve creation order without SQLite timestamp conversion.
            var entries = await query.OrderByDescending(e => e.Id).Skip(offset).Take(page.PageSize).ToListAsync();
            return Results.Ok(new Page<AuditEntry>(entries, entries.Count == page.PageSize ? (offset + entries.Count).ToString(System.Globalization.CultureInfo.InvariantCulture) : null));
        }).WithMetadata(new AccessScope("R"));
    }
}
