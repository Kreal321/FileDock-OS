using FileDock.Management.Api.AdminSocket;
using FileDock.Management.Api.Auth;
using FileDock.Management.Api.Endpoints;
using FileDock.Management.Api.Infrastructure;
using FileDock.Management.Api.Middleware;
using FileDock.Management.Core;
using FileDock.Management.Data;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Security;
using FileDock.Management.Domain.Sessions;
using FileDock.Management.Domain.Setup;
using FileDock.Management.Domain.Stores;
using Microsoft.AspNetCore.Authentication;
using Microsoft.EntityFrameworkCore;

var builder = WebApplication.CreateBuilder(args);

builder.Services.AddFileDockProblemDetails();
builder.Services.AddHttpContextAccessor();
builder.Services.AddSingleton<PasswordHasher>();
builder.Services.AddScoped<IAuditStore, EfAuditStore>();
builder.Services.AddScoped<ISetupStore, EfSetupStore>();
builder.Services.AddScoped<IAdminStore, EfAdminStore>();
builder.Services.AddScoped<ISessionStore, EfSessionStore>();
builder.Services.AddScoped<AuditWriter>();
builder.Services.AddScoped<SetupCodeService>();
builder.Services.AddScoped<AdminService>();
builder.Services.AddScoped<SessionService>();
builder.Services.AddHostedService<AuditRetention>();
if (builder.Configuration.GetValue("Core:Optional", false))
{
    builder.Services.AddSingleton<ICoreGateway, LocalCoreGateway>();
}
else
{
    builder.Services.AddSingleton<CoreClient>();
    builder.Services.AddSingleton<ICoreGateway>(sp => sp.GetRequiredService<CoreClient>());
}

builder.Services.AddAuthentication(SessionAuthHandler.SchemeName)
    .AddScheme<AuthenticationSchemeOptions, SessionAuthHandler>(SessionAuthHandler.SchemeName, _ => { });
builder.Services.AddAuthorization();
builder.Services.AddDbContext<AppDbContext>(options =>
{
    var path = builder.Configuration["Database:Path"] ?? "app.db";
    options.UseSqlite($"Data Source={path}");
});

builder.WebHost.ConfigureKestrel(options =>
{
    var httpPort = builder.Configuration.GetValue("Kestrel:HttpPort", 8021);
    options.ListenAnyIP(httpPort);
    var adminSocket = builder.Configuration["AdminSocket:Path"];
    if (!string.IsNullOrEmpty(adminSocket))
    {
        options.ListenUnixSocket(adminSocket);
    }
});

var app = builder.Build();
app.UseFileDockProblemDetails();
app.UseAuthentication();
app.UseAuthorization();
app.UseMiddleware<SetupLockMiddleware>();
app.Use(async (context, next) =>
{
    if (!SessionAuthHandler.CsrfValid(context))
    {
        context.Response.StatusCode = 403;
        await context.Response.WriteAsJsonAsync(new { title = "CSRF", status = 403, detail = "CSRF token missing or invalid." });
        return;
    }
    await next(context);
});

using (var scope = app.Services.CreateScope())
{
    var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
    db.Database.Migrate();
}

var api = app.MapGroup("/api/v1");
api.MapGet("/health", () => Results.Ok(new { status = "ok" }));
app.MapGet("/health", () => Results.Ok(new { status = "ok" }));
api.MapSetup();
api.MapAuth();
api.MapSystem();
AdminSocketEndpoints.Map(app);

app.UseDefaultFiles();
app.UseStaticFiles();

var adminSocketPath = builder.Configuration["AdminSocket:Path"];
app.Lifetime.ApplicationStarted.Register(() =>
{
    if (!string.IsNullOrEmpty(adminSocketPath) && !OperatingSystem.IsWindows())
    {
        File.SetUnixFileMode(adminSocketPath, UnixFileMode.UserRead | UnixFileMode.UserWrite);
    }
});

app.Run();

public partial class Program;
