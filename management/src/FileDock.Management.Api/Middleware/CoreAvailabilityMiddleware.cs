using FileDock.Management.Core;

namespace FileDock.Management.Api.Middleware;

public sealed class CoreAvailabilityMiddleware(RequestDelegate next)
{
    public async Task InvokeAsync(HttpContext context, CoreHealthMonitor monitor)
    {
        var path = context.Request.Path.Value ?? "";
        if (path.StartsWith("/api/v1/", StringComparison.OrdinalIgnoreCase)
            && !HttpMethods.IsGet(context.Request.Method) && !HttpMethods.IsHead(context.Request.Method)
            && !path.StartsWith("/api/v1/auth/", StringComparison.OrdinalIgnoreCase)
            && !path.StartsWith("/api/v1/setup/", StringComparison.OrdinalIgnoreCase)
            && !monitor.Current.CanMutate)
        {
            await Results.Problem(statusCode: 503, title: "Core unavailable", detail: monitor.Current.Detail,
                extensions: new Dictionary<string, object?> { ["code"] = monitor.Current.State == "Incompatible" ? "core.interface_mismatch" : "core.unreachable" }).ExecuteAsync(context);
            return;
        }
        await next(context);
    }
}
