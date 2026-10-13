using FileDock.Management.Domain.Setup;

namespace FileDock.Management.Api.Middleware;

public sealed class SetupLockMiddleware(RequestDelegate next)
{
    public async Task InvokeAsync(HttpContext context, SetupCodeService setup)
    {
        var path = context.Request.Path.Value ?? "";
        if (IsOpen(path) || await setup.IsCompleteAsync(context.RequestAborted))
        {
            await next(context);
            return;
        }

        context.Response.StatusCode = StatusCodes.Status423Locked;
        context.Response.ContentType = "application/problem+json";
        await context.Response.WriteAsJsonAsync(new
        {
            type = "about:blank",
            title = "Setup required",
            status = 423,
            detail = "Finish first-run setup before using this appliance.",
        });
    }

    private static bool IsOpen(string path)
    {
        return path == "/"
            || path.StartsWith("/assets/", StringComparison.OrdinalIgnoreCase)
            || path.Equals("/favicon.ico", StringComparison.OrdinalIgnoreCase)
            || path.StartsWith("/setup", StringComparison.OrdinalIgnoreCase)
            || path.StartsWith("/api/v1/setup", StringComparison.OrdinalIgnoreCase)
            || path.Equals("/api/v1/auth/sign-in", StringComparison.OrdinalIgnoreCase)
            || path.Equals("/health", StringComparison.OrdinalIgnoreCase)
            || path.Equals("/api/v1/health", StringComparison.OrdinalIgnoreCase)
            || path.StartsWith("/admin", StringComparison.OrdinalIgnoreCase);
    }
}
