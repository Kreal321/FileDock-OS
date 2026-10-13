using FileDock.Management.Api.AdminSocket;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Tokens;

namespace FileDock.Management.Api.Auth;

public sealed record AccessScope(string Value);

public sealed class ScopePolicies(RequestDelegate next)
{
    public async Task InvokeAsync(HttpContext context, AuditWriter audit, ApiTokenService tokens)
    {
        var path = context.Request.Path.Value ?? "";
        if ((path.Equals("/admin", StringComparison.OrdinalIgnoreCase) || path.StartsWith("/admin/", StringComparison.OrdinalIgnoreCase))
            && !AdminSocketEndpoints.IsLocalAdmin(context))
        {
            await audit.WriteAsync("System", "anonymous", "anonymous", "auth.denied", path, "Refused", new { status = 403 });
            await Results.Json(new { title = "Forbidden", status = 403, detail = "This endpoint requires the local console socket." }, contentType: "application/problem+json", statusCode: 403).ExecuteAsync(context);
            return;
        }
        var isHub = path.Equals("/hubs/events", StringComparison.OrdinalIgnoreCase) || path.StartsWith("/hubs/events/", StringComparison.OrdinalIgnoreCase);
        if (!isHub && !path.StartsWith("/api/v1/", StringComparison.OrdinalIgnoreCase)) { await next(context); return; }
        var token = context.Items["fd.token"] as ApiToken;
        var admin = context.Items["fd.admin"] as Administrator;
        var scope = isHub ? "R" : context.GetEndpoint()?.Metadata.GetMetadata<AccessScope>()?.Value
            ?? (HttpMethods.IsGet(context.Request.Method) || HttpMethods.IsHead(context.Request.Method) ? "R" : "F");
        var isPublic = path.Equals("/api/v1/health", StringComparison.OrdinalIgnoreCase)
            || path.Equals("/api/v1/auth/sign-in", StringComparison.OrdinalIgnoreCase)
            || scope == "P";
        if (token is not null)
        {
            await tokens.AuditUseAsync(token, context.Request.Method, path);
        }

        if (token is not null && (scope is "S" or "P" || scope == "F" && token.Scope != "Full"))
        {
            await Deny(403, "auth.denied", "This credential does not allow this action."); return;
        }
        if (!isPublic && context.User.Identity?.IsAuthenticated != true)
        {
            await Deny(401, "auth.denied", "Sign in or supply a valid API token."); return;
        }
        if (!isHub && !SessionAuthHandler.CsrfValid(context))
        {
            await Deny(403, "auth.csrf", "CSRF token missing or invalid."); return;
        }
        await next(context);
        if (context.Response.StatusCode is 401 or 403)
        {
            await AuditDenial("auth.denied");
        }
        else if (context.Response.StatusCode >= 400 && !HttpMethods.IsGet(context.Request.Method) && !HttpMethods.IsHead(context.Request.Method))
        {
            await audit.WriteAsync(token is not null ? "ApiToken" : admin is not null ? "Administrator" : "System",
                token?.Id.ToString() ?? admin?.Id.ToString() ?? "anonymous", token?.Name ?? admin?.Username ?? "anonymous",
                "request.failed", path, "Failed", new { method = context.Request.Method, status = context.Response.StatusCode });
        }

        async Task AuditDenial(string action) => await audit.WriteAsync(
            token is not null ? "ApiToken" : admin is not null ? "Administrator" : "System",
            token?.Id.ToString() ?? admin?.Id.ToString() ?? "anonymous",
            token?.Name ?? admin?.Username ?? "anonymous", action, path, "Refused",
            new { method = context.Request.Method, status = context.Response.StatusCode });
        async Task Deny(int status, string action, string detail)
        {
            context.Response.StatusCode = status;
            await AuditDenial(action);
            await Results.Json(new { type = "about:blank", title = status == 401 ? "Unauthorized" : "Forbidden", status, detail }, contentType: "application/problem+json", statusCode: status).ExecuteAsync(context);
        }
    }
}
