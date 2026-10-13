using FileDock.Management.Api.AdminSocket;
using FileDock.Management.Api.Auth;
using FileDock.Management.Api.Endpoints;
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
if (builder.Configuration.GetValue("Core:Optional", false))
{
    builder.Services.AddSingleton<ICoreGateway, LocalCoreGateway>();
}
else
{
    builder.Services.AddSingleton<CoreClient>();
    builder.Services.AddSingleton<ICoreGateway>(sp => sp.GetRequiredService<CoreClient>());
}

builder.Services.AddAuthentication("fd-auth")
    .AddPolicyScheme("fd-auth", "Session or API token", options =>
        options.ForwardDefaultSelector = context => context.Request.Headers.ContainsKey("Authorization")
            ? TokenAuthHandler.SchemeName : SessionAuthHandler.SchemeName)
    .AddScheme<AuthenticationSchemeOptions, SessionAuthHandler>(SessionAuthHandler.SchemeName, _ => { })
    .AddScheme<AuthenticationSchemeOptions, TokenAuthHandler>(TokenAuthHandler.SchemeName, _ => { });
builder.Services.AddAuthorization();
builder.Services.AddDbContext<AppDbContext>(options =>
{
    var path = builder.Configuration["Database:Path"] ?? "app.db";
    Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
    options.UseSqlite($"Data Source={path}");
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
