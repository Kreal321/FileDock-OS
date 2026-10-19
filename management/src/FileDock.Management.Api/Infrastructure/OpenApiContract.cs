using FileDock.Management.Api.Auth;
using Microsoft.AspNetCore.OpenApi;
using Microsoft.OpenApi;

namespace FileDock.Management.Api.Infrastructure;

public static class OpenApiContract
{
    public static IServiceCollection AddFileDockOpenApi(this IServiceCollection services)
    {
        services.AddOpenApi("v1", options =>
        {
            options.ShouldInclude = d => d.RelativePath?.StartsWith("api/v1/", StringComparison.Ordinal) == true;
            options.AddDocumentTransformer((document, _, _) =>
            {
                document.Info.Title = "FileDock Management API";
                document.Info.Version = "1";
                document.Info.Description = "Single-appliance HTTP API. Session mutations require X-CSRF-Token; API tokens use the Authorization Bearer header. Errors use application/problem+json. Recovery is to the latest durable point; later changes may be lost.";
                return Task.CompletedTask;
            });
            options.AddOperationTransformer((operation, context, _) =>
            {
                var path = "/" + context.Description.RelativePath?.Split('?')[0];
                var scope = context.Description.ActionDescriptor.EndpointMetadata.OfType<AccessScope>().LastOrDefault()?.Value ?? "R";
                var mutation = context.Description.HttpMethod is not ("GET" or "HEAD");
                var open = path is "/api/v1/health" or "/api/v1/auth/sign-in" || path.StartsWith("/api/v1/setup/", StringComparison.Ordinal);
                operation.Description = $"{operation.Description}\nAccess scope: {scope}.".Trim();
                operation.Responses ??= new OpenApiResponses();
                void Error(string code, string description) => operation.Responses.TryAdd(code, new OpenApiResponse
                {
                    Description = description,
                    Content = new Dictionary<string, OpenApiMediaType> { ["application/problem+json"] = new() }
                });
                Error("400", "Malformed request or invalid input.");
                Error("500", "Unexpected server failure; includes a traceId.");
                if (!open) { Error("401", "Authentication required."); Error("403", "Scope or CSRF check refused."); Error("423", "Finish first-run setup."); }
                if (mutation)
                {
                    Error("507", "storage.disk_full: change was not committed.");
                    if (!path.StartsWith("/api/v1/auth/", StringComparison.Ordinal) && !path.StartsWith("/api/v1/setup/", StringComparison.Ordinal))
                    {
                        Error("503", "core.unreachable or core.interface_mismatch; changes disabled.");
                        Error("409", "Generation conflict or resource in use.");
                        Error("422", "Domain validation or precondition refused.");
                    }
                    if (context.Description.HttpMethod is "PATCH" or "PUT" or "DELETE") { Error("428", "Supply the current generation using If-Match where required."); }
                }
                if (path.StartsWith("/api/v1/setup/", StringComparison.Ordinal) || path == "/api/v1/auth/sign-in")
                {
                    Error("401", "Credentials, recovery key or setup code refused.");
                    Error("403", "Setup session or token scope refused.");
                    Error("409", "Setup or recovery conflicts with current state.");
                    Error("422", "Setup or recovery validation failed.");
                    Error("429", "Too many attempts; retry after the specified delay.");
                    Error("503", "Core unavailable for setup or recovery.");
                }
                if (path.Contains("/{", StringComparison.Ordinal)) { Error("404", "Resource not found."); }
                return Task.CompletedTask;
            });
        });
        return services;
    }
}
