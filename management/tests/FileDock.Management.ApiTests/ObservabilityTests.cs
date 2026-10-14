using System.Net;
using System.Net.Http.Json;
using System.Text.Json;
using FileDock.Management.Domain.Admins;
using FileDock.Management.Domain.Setup;
using FileDock.Management.Domain.Tokens;
using FileDock.Management.Domain.Audit;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.Extensions.DependencyInjection;
namespace FileDock.Management.ApiTests;

public sealed class ObservabilityTests
{
    [Fact]
    public async Task AuditFiltersAreNewestFirstAndSensitiveExportsRequireFullScope()
    {
        using var f = new WebApplicationFactory<Program>().WithWebHostBuilder(b => { b.UseSetting("Core:Optional", "true"); b.UseSetting("Database:Path", Path.Combine(Path.GetTempPath(), "fd-observe-" + Guid.NewGuid().ToString("N"), "app.db")); b.UseSetting("Kestrel:HttpPort", "0"); b.UseSetting("Logging:LogLevel:Default", "Warning"); });
        using var c = f.CreateClient(); using var scope = f.Services.CreateScope(); var admin = await scope.ServiceProvider.GetRequiredService<AdminService>().CreateAsync("observe-admin", "test-only"); await scope.ServiceProvider.GetRequiredService<SetupCodeService>().MarkCompleteAsync(); var token = await scope.ServiceProvider.GetRequiredService<ApiTokenService>().CreateAsync("read", "ReadOnly", admin.Id, null); c.DefaultRequestHeaders.Authorization = new("Bearer", token.Token);
        var audit = scope.ServiceProvider.GetRequiredService<AuditWriter>(); await audit.WriteAsync("Administrator", "test", "audit-tester", "test.first", "target", "Succeeded", new { password = "CANARY-DO-NOT-EXPOSE" }); await audit.WriteAsync("Administrator", "test", "audit-tester", "test.second", "target", "Succeeded");
        var response = await c.GetAsync("/api/v1/audit?actor=audit-tester&result=Succeeded&since=2020-01-01T00%3A00%3A00Z"); Assert.Equal(HttpStatusCode.OK, response.StatusCode); var text = await response.Content.ReadAsStringAsync(); Assert.DoesNotContain("CANARY-DO-NOT-EXPOSE", text); using var body = JsonDocument.Parse(text); var rows = body.RootElement.GetProperty("items").EnumerateArray().ToArray(); Assert.Equal(2, rows.Length); Assert.Equal("test.second", rows[0].GetProperty("action").GetString());
        Assert.Equal(HttpStatusCode.BadRequest, (await c.GetAsync("/api/v1/audit?since=invalid")).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await c.GetAsync("/api/v1/diagnostics/bundle")).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await c.PostAsJsonAsync("/api/v1/alerts/fake/ack", new { })).StatusCode);
        Assert.Equal(HttpStatusCode.Forbidden, (await c.PostAsJsonAsync("/api/v1/notification-channels", new { name = "test", subsystemId = "notify-test", minSeverity = "Warning", config = new { } })).StatusCode);
    }
}
