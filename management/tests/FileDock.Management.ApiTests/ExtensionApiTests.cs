using System.Net;
using System.Net.Http.Json;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Setup;
using FileDock.Management.Domain.Sessions;
using FileDock.Management.Domain.Tokens;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.Extensions.DependencyInjection;
namespace FileDock.Management.ApiTests;

public sealed class ExtensionApiTests
{
    [Fact]
    public async Task ExtensionMutationsRequireFullScopeAndRejectUnsafeStagedPaths()
    {
        using var factory = new WebApplicationFactory<Program>().WithWebHostBuilder(builder =>
        {
            builder.UseSetting("Core:Optional", "true"); builder.UseSetting("Database:Path", Path.Combine(Path.GetTempPath(), $"fd-extensions-{Guid.NewGuid():N}", "app.db")); builder.UseSetting("Kestrel:HttpPort", "0"); builder.UseSetting("Logging:LogLevel:Default", "Warning");
        });
        using var client = factory.CreateClient(new WebApplicationFactoryClientOptions { HandleCookies = false });
        Assert.Equal(HttpStatusCode.Locked, (await client.GetAsync("/api/v1/subsystems")).StatusCode);
        using var scope = factory.Services.CreateScope(); var admin = await scope.ServiceProvider.GetRequiredService<AdminService>().CreateAsync("extension-admin", "test-only"); await scope.ServiceProvider.GetRequiredService<SetupCodeService>().MarkCompleteAsync();
        var token = await scope.ServiceProvider.GetRequiredService<ApiTokenService>().CreateAsync("read", "ReadOnly", admin.Id, null);
        client.DefaultRequestHeaders.Authorization = new("Bearer", token.Token);
        Assert.Equal(HttpStatusCode.Forbidden, (await client.PostAsJsonAsync("/api/v1/extensions", new { stagedPackageId = "../../etc/shadow", approvedPermissions = Array.Empty<string>(), acknowledgedUnsigned = true, dataBindings = new Dictionary<string, string>(), acknowledgedUnprotectedData = false })).StatusCode);
        var session = await scope.ServiceProvider.GetRequiredService<SessionService>().CreateAsync(admin, "127.0.0.1", "test"); client.DefaultRequestHeaders.Authorization = null; client.DefaultRequestHeaders.Add("Cookie", $"fd_http_session={session.Token}"); client.DefaultRequestHeaders.Add("X-CSRF-Token", session.Csrf);
        Assert.Equal(HttpStatusCode.BadRequest, (await client.PostAsJsonAsync("/api/v1/extensions", new { stagedPackageId = "../../etc/shadow", approvedPermissions = Array.Empty<string>(), acknowledgedUnsigned = true, dataBindings = new Dictionary<string, string>(), acknowledgedUnprotectedData = false })).StatusCode);
    }
}
