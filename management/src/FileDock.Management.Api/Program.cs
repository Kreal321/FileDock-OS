using FileDock.Management.Api.AdminSocket;
using FileDock.Management.Api.Infrastructure;
using FileDock.Management.Core;
using FileDock.Management.Data;
using FileDock.Management.Domain.Audit;
using FileDock.Management.Domain.Security;
using Microsoft.EntityFrameworkCore;

var builder = WebApplication.CreateBuilder(args);

builder.Services.AddFileDockProblemDetails();
builder.Services.AddHttpContextAccessor();
builder.Services.AddSingleton<PasswordHasher>();
builder.Services.AddScoped<IAuditStore, EfAuditStore>();
builder.Services.AddScoped<AuditWriter>();
builder.Services.AddHostedService<AuditRetention>();
builder.Services.AddSingleton<CoreClient>();
builder.Services.AddDbContext<AppDbContext>(options =>
{
    var path = builder.Configuration["Database:Path"] ?? "app.db";
    options.UseSqlite($"Data Source={path}");
});

builder.WebHost.ConfigureKestrel(options =>
{
    var httpsPort = builder.Configuration.GetValue("Kestrel:HttpsPort", 8443);
    options.ListenAnyIP(httpsPort, listen =>
    {
        if (builder.Configuration.GetValue("Kestrel:UseHttps", false))
        {
            listen.UseHttps();
        }
    });
    var adminSocket = builder.Configuration["AdminSocket:Path"];
    if (!string.IsNullOrEmpty(adminSocket))
    {
        options.ListenUnixSocket(adminSocket);
    }
});

var app = builder.Build();
app.UseFileDockProblemDetails();

using (var scope = app.Services.CreateScope())
{
    var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
    db.Database.EnsureCreated();
}

var api = app.MapGroup("/api/v1");
api.MapGet("/health", () => Results.Ok(new { status = "ok" }));
app.MapGet("/health", () => Results.Ok(new { status = "ok" }));
AdminSocketEndpoints.Map(app);

app.UseDefaultFiles();
app.UseStaticFiles();

app.Run();

public partial class Program;
