using System.Text.Json;
using Microsoft.AspNetCore.Diagnostics;
using Microsoft.AspNetCore.Mvc;

namespace FileDock.Management.Api.Infrastructure;

public static class ProblemDetailsExtensions
{
    public static IServiceCollection AddFileDockProblemDetails(this IServiceCollection services)
    {
        services.AddProblemDetails(options =>
        {
            options.CustomizeProblemDetails = context =>
            {
                context.ProblemDetails.Extensions.TryAdd("traceId", context.HttpContext.TraceIdentifier);
            };
        });
        return services;
    }

    public static IApplicationBuilder UseFileDockProblemDetails(this IApplicationBuilder app)
    {
        app.UseExceptionHandler(handler =>
        {
            handler.Run(async context =>
            {
                var feature = context.Features.Get<IExceptionHandlerFeature>();
                var problem = new ProblemDetails
                {
                    Status = StatusCodes.Status500InternalServerError,
                    Title = "An unexpected error occurred",
                    Type = "about:blank",
                    Detail = feature?.Error.Message,
                };
                problem.Extensions["traceId"] = context.TraceIdentifier;
                context.Response.StatusCode = problem.Status.Value;
                context.Response.ContentType = "application/problem+json";
                await context.Response.WriteAsync(JsonSerializer.Serialize(problem));
            });
        });
        return app;
    }
}

public sealed record ValidationProblemError(string Code, string Field, string Message, string? Conflicting);
