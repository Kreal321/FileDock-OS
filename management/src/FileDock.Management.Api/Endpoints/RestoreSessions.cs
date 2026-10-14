using System.Collections.Concurrent;
using System.Security.Cryptography;
namespace FileDock.Management.Api.Endpoints;
/// Browser-bound restore authorization. Neither setup codes nor recovery keys are retained.
public sealed class RestoreSessions
{
    private sealed record Entry(DateTimeOffset Expires, string SourceId, string? OperationId);
    private readonly ConcurrentDictionary<string, Entry> sessions = new();
    public void Issue(HttpContext context, string sourceId)
    {
        foreach (var pair in sessions.Where(p => p.Value.Expires <= DateTimeOffset.UtcNow))
        {
            sessions.TryRemove(pair.Key, out _);
        }

        if (sessions.Count >= 100)
        {
            throw new InvalidOperationException("Too many restore sessions.");
        }

        var token = Convert.ToHexString(RandomNumberGenerator.GetBytes(32));
        sessions[token] = new(DateTimeOffset.UtcNow.AddMinutes(15), sourceId, null);
        context.Response.Cookies.Append("fd-restore", token, new CookieOptions { HttpOnly = true, SameSite = SameSiteMode.Strict, Secure = false, Path = "/api/v1/setup/restore", MaxAge = TimeSpan.FromMinutes(15) });
    }
    public string Source(HttpContext context)
    {
        if (!context.Request.Cookies.TryGetValue("fd-restore", out var token) || !sessions.TryGetValue(token, out var entry) || entry.Expires <= DateTimeOffset.UtcNow)
        {
            throw new UnauthorizedAccessException("Restore session expired. Open the source again.");
        }

        return entry.SourceId;
    }
    public void BindOperation(HttpContext context, string operationId)
    {
        var source = Source(context); var token = context.Request.Cookies["fd-restore"]!;
        sessions[token] = sessions[token] with { OperationId = operationId };
    }
    public void CheckOperation(HttpContext context, string operationId)
    {
        Source(context); var token = context.Request.Cookies["fd-restore"]!;
        if (sessions[token].OperationId != operationId)
        {
            throw new UnauthorizedAccessException("Restore operation does not match this session.");
        }
    }
}
