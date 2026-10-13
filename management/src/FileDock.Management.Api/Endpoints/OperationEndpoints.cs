using FileDock.Management.Api.Auth;
using FileDock.Management.Core;
using Grpc.Core;
using FileDock.Management.Domain.Audit;
using System.Security.Claims;

namespace FileDock.Management.Api.Endpoints;

public static class OperationEndpoints
{
    public static RouteGroupBuilder MapOperations(this RouteGroupBuilder api)
    {
        api.MapGet("/operations", async (bool? activeOnly, uint? pageSize, string? pageToken, IOperationGateway core, CancellationToken ct) =>
        {
            if (pageSize is 0) { return Results.Problem(statusCode: 400, title: "Invalid page size"); }
            try { return Results.Ok(await core.ListOperationsAsync(activeOnly ?? false, pageSize ?? 100, pageToken, ct)); }
            catch (RpcException error) { return Problem(error); }
        }).WithMetadata(new AccessScope("R"));
        api.MapGet("/operations/{id}", async (string id, IOperationGateway core, CancellationToken ct) =>
        {
            try { return Results.Ok(await core.GetOperationAsync(id, ct)); }
            catch (RpcException error) { return Problem(error); }
        }).WithMetadata(new AccessScope("R"));
        api.MapPost("/operations/reconcile", async (HttpContext context, IOperationGateway core, AuditWriter audit, CancellationToken ct) =>
        {
            try
            {
                var operation = await core.ReconcileAllAsync(context.User.Identity?.Name ?? "unknown", context.TraceIdentifier, ct);
                await audit.WriteAsync(context.Items["fd.token"] is not null ? "ApiToken" : "Administrator", context.User.FindFirstValue(ClaimTypes.NameIdentifier) ?? "unknown", context.User.Identity?.Name ?? "unknown", "operation.started", operation.Id, "Succeeded", new { operation.Kind }, CancellationToken.None);
                return Results.Accepted($"/api/v1/operations/{operation.Id}", new { operationId = operation.Id });
            }
            catch (RpcException error) { return Problem(error); }
        }).WithMetadata(new AccessScope("F"));
        return api;
    }
    private static IResult Problem(RpcException error)
    {
        var (status, detail) = CoreClient.ToProblem(error);
        return Results.Problem(statusCode: status, title: "Core operation request failed", detail: detail);
    }
}
