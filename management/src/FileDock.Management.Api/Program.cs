using FileDock.Management.Api.AdminSocket;
using FileDock.Management.Api.Auth;
using FileDock.Management.Api.Endpoints;
using FileDock.Management.Api.Hubs;
using FileDock.Management.Api.Infrastructure;
using FileDock.Management.Api.Middleware;
using FileDock.Management.Api.Network;
using FileDock.Management.Core;
using FileDock.Management.Data;
using FileDock.Management.Domain;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Security;
using FileDock.Management.Domain.Sessions;
using FileDock.Management.Domain.Setup;
using FileDock.Management.Domain.Stores;
using FileDock.Management.Domain.Tokens;
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
builder.Services.AddScoped<IApiTokenStore, EfApiTokenStore>();
builder.Services.AddScoped<ApiTokenService>();
builder.Services.AddHostedService<AuditRetention>();
builder.Services.AddScoped<FileDock.Management.Domain.Restore.IManagementConfigStore, ManagementConfigStore>();
builder.Services.AddSingleton<ManagementConfigChangeInterceptor>();
if (!builder.Configuration.GetValue("Core:Optional", false))
{
    builder.Services.AddHostedService<ManagementConfigPublisher>();
}

builder.Services.AddSingleton<RestoreSessions>();
builder.Services.AddSingleton<CoreClient>();
if (builder.Configuration.GetValue("Core:Optional", false))
{
    builder.Services.AddSingleton<LocalCoreGateway>();
    builder.Services.AddSingleton<ICoreGateway>(sp => sp.GetRequiredService<LocalCoreGateway>());
    builder.Services.AddSingleton<ICoreEventSource>(sp => sp.GetRequiredService<LocalCoreGateway>());
    builder.Services.AddSingleton<IOperationGateway>(sp => sp.GetRequiredService<LocalCoreGateway>());
}
else
{
    builder.Services.AddSingleton<ICoreGateway>(sp => sp.GetRequiredService<CoreClient>());
    builder.Services.AddSingleton<ICoreEventSource>(sp => sp.GetRequiredService<CoreClient>());
    builder.Services.AddSingleton<IOperationGateway>(sp => sp.GetRequiredService<CoreClient>());
}

builder.Services.AddSignalR();
builder.Services.AddSingleton<EventConnections>();
builder.Services.AddHostedService(sp => sp.GetRequiredService<EventConnections>());
builder.Services.AddSingleton<IManagementEventSink, ManagementEventSink>();
builder.Services.AddSingleton<CoreHealthMonitor>();
builder.Services.AddHostedService(sp => sp.GetRequiredService<CoreHealthMonitor>());
builder.Services.AddHostedService<EventRelay>();

builder.Services.AddAuthentication("fd-auth")
    .AddPolicyScheme("fd-auth", "Session or API token", options =>
        options.ForwardDefaultSelector = context => context.Request.Headers.ContainsKey("Authorization")
            ? TokenAuthHandler.SchemeName : SessionAuthHandler.SchemeName)
    .AddScheme<AuthenticationSchemeOptions, SessionAuthHandler>(SessionAuthHandler.SchemeName, _ => { })
    .AddScheme<AuthenticationSchemeOptions, TokenAuthHandler>(TokenAuthHandler.SchemeName, _ => { });
builder.Services.AddAuthorization();
builder.Services.AddDbContext<AppDbContext>((sp, options) =>
{
    var path = builder.Configuration["Database:Path"] ?? "app.db";
    Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
    options.UseSqlite($"Data Source={path}").AddInterceptors(sp.GetRequiredService<ManagementConfigChangeInterceptor>());
});

var listener = new ListenerConfiguration(builder.Configuration.GetValue("Kestrel:HttpPort", 8021));
listener.Apply(NetworkService.DefaultAddresses);
var listenerRoot = new ConfigurationBuilder().Add(listener).Build();
builder.Services.AddSingleton(listener);
builder.Services.AddSingleton<IConfigurationRoot>(listenerRoot);
builder.Services.AddScoped<NetworkService>();

builder.WebHost.ConfigureKestrel(options =>
{
    options.Configure(listenerRoot.GetSection("Kestrel"), reloadOnChange: true);
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
app.UseMiddleware<ScopePolicies>();
app.UseMiddleware<CoreAvailabilityMiddleware>();
// WebSocket credentials are cookies; reject foreign browser origins before upgrading.
app.Use(async (context, next) =>
{
    if (context.Request.Path.StartsWithSegments("/hubs/events") && context.Request.Headers.TryGetValue("Origin", out var origin)
        && (!Uri.TryCreate(origin.ToString(), UriKind.Absolute, out var uri) || !string.Equals(uri.Authority, context.Request.Host.Value, StringComparison.OrdinalIgnoreCase) || uri.Scheme != context.Request.Scheme))
    {
        await Results.Problem(statusCode: 403, title: "Foreign event-stream origin refused").ExecuteAsync(context); return;
    }
    await next(context);
});

using (var scope = app.Services.CreateScope())
{
    var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
    db.Database.Migrate();
    listener.Apply(scope.ServiceProvider.GetRequiredService<NetworkService>().GetBindableAsync().GetAwaiter().GetResult());
}

var api = app.MapGroup("/api/v1");
api.MapGet("/health", () => Results.Ok(new { status = "ok" }));
app.MapGet("/health", () => Results.Ok(new { status = "ok" }));
api.MapSetup();
api.MapAuth();
api.MapSystem();
api.MapAdmins();
api.MapTokens();
api.MapNetwork();
api.MapAudit();
api.MapOperations();
api.MapRecovery();
api.MapExtensions();
api.MapObservability();
api.MapFolders();
api.MapStorage();
api.MapShares();
api.MapApps();
api.MapConnections();
api.MapConnectors();
api.MapGet("/system/core-health", (CoreHealthMonitor monitor) => Results.Ok(monitor.Current)).WithMetadata(new AccessScope("R"));
app.MapHub<EventsHub>("/hubs/events");
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
