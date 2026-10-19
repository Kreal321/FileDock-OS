using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.Extensions.Logging;

namespace FileDock.Management.Domain.Audit;

public sealed partial class AuditWriter(IAuditStore store, ILogger<AuditWriter> logger)
{
    public async Task WriteAsync(
        string actorKind,
        string actorId,
        string actorName,
        string action,
        string target,
        string result,
        object? detail = null,
        Guid? auditEntryId = null,
        CancellationToken cancellationToken = default)
    {
        await store.AddAsync(
            new AuditEntry
            {
                Id = auditEntryId ?? Guid.CreateVersion7(),
                At = DateTimeOffset.UtcNow,
                ActorKind = actorKind,
                ActorId = actorId,
                ActorName = actorName,
                Action = action,
                Target = target,
                Result = result,
                Detail = Redact(detail),
            },
            cancellationToken);
        LogAudit(logger, action, result, target);
    }

    [LoggerMessage(EventId = 1, Level = LogLevel.Debug, Message = "audit {Action} {Result} {Target}")]
    private static partial void LogAudit(ILogger logger, string action, string result, string target);

    public static string Redact(object? detail)
    {
        if (detail is null)
        {
            return "{}";
        }

        var node = JsonSerializer.SerializeToNode(detail) ?? new JsonObject();
        RedactNode(node);
        return node.ToJsonString();
    }

    private static void RedactNode(JsonNode? node)
    {
        if (node is JsonObject obj)
        {
            foreach (var key in obj.Select(p => p.Key).ToList())
            {
                if (IsSecret(key))
                {
                    obj[key] = "••••";
                }
                else
                {
                    RedactNode(obj[key]);
                }
            }
        }
        else if (node is JsonArray array)
        {
            foreach (var child in array)
            {
                RedactNode(child);
            }
        }
    }

    private static bool IsSecret(string key)
    {
        var compact = key.Replace("-", "", StringComparison.Ordinal).Replace("_", "", StringComparison.Ordinal);
        return compact.Contains("password", StringComparison.OrdinalIgnoreCase)
            || compact.Contains("secret", StringComparison.OrdinalIgnoreCase)
            || compact.Contains("token", StringComparison.OrdinalIgnoreCase)
            || compact.Contains("credential", StringComparison.OrdinalIgnoreCase);
    }
}
