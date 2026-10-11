namespace FileDock.Management.Api.Infrastructure;

public static class Concurrency
{
    public static long? IfMatch(HttpRequest request)
    {
        if (!request.Headers.TryGetValue("If-Match", out var values))
        {
            return null;
        }

        var raw = values.ToString().Trim().Trim('"');
        return long.TryParse(raw, out var generation) ? generation : null;
    }

    public static void SetGeneration(HttpResponse response, long generation)
    {
        response.Headers.ETag = $"\"{generation}\"";
        response.Headers.Append("X-Generation", generation.ToString());
    }
}

public sealed record PageQuery(int PageSize, string? PageToken)
{
    public static PageQuery From(HttpRequest request)
    {
        var size = 100;
        if (int.TryParse(request.Query["pageSize"], out var parsed) && parsed > 0)
        {
            size = parsed;
        }

        var token = request.Query["pageToken"].ToString();
        return new PageQuery(size, string.IsNullOrEmpty(token) ? null : token);
    }
}

public sealed record Page<T>(IReadOnlyList<T> Items, string? NextPageToken);
