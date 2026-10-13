using System.Net;
using System.Net.Http.Json;
using FileDock.Management.Core;
using FileDock.Management.Data;
using Grpc.Core;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;

namespace FileDock.Management.ApiTests;

public sealed class FirstRunTests : IClassFixture<WebApplicationFactory<Program>>
{
    private readonly WebApplicationFactory<Program> _factory;

    public FirstRunTests(WebApplicationFactory<Program> factory)
    {
        _factory = factory.WithWebHostBuilder(builder =>
        {
            builder.UseSetting("Core:Optional", "true");
            builder.UseSetting("Database:Path", Path.Combine(Path.GetTempPath(), $"fd-api-{Guid.NewGuid():N}.db"));
            builder.UseSetting("Kestrel:HttpPort", "0");
        });
    }

    [Fact]
    public async Task OtherRoutesAreLockedUntilSetupCompletes()
    {
        var client = _factory.CreateClient();
        var locked = await client.GetAsync("/api/v1/system/status");
        Assert.Equal(HttpStatusCode.Locked, locked.StatusCode);

        var health = await client.GetAsync("/health");
        Assert.True(health.IsSuccessStatusCode);
    }

    [Fact]
    public async Task SetupCompletesOnlyAfterRecoveryKeyConfirmation()
    {
        var client = _factory.CreateClient();
        var issued = await client.PostAsync("/admin/setup-code", null);
        issued.EnsureSuccessStatusCode();
        var code = (await issued.Content.ReadFromJsonAsync<CodeResponse>())!.Code;

        var complete = await client.PostAsJsonAsync("/api/v1/setup/complete", new
        {
            code,
            applianceName = "harbor",
            adminUsername = "alice",
            adminPassword = "correct horse",
            adminPasswordConfirmation = "correct horse",
        });
        complete.EnsureSuccessStatusCode();
        var generated = await complete.Content.ReadFromJsonAsync<GeneratedResponse>();
        Assert.False(string.IsNullOrEmpty(generated?.RecoveryKey));

        var stillLocked = await client.GetAsync("/api/v1/system/status");
        Assert.Equal(HttpStatusCode.Locked, stillLocked.StatusCode);

        using var confirmRequest = new HttpRequestMessage(HttpMethod.Post, "/api/v1/setup/recovery-key/confirm")
        {
            Content = JsonContent.Create(new { keyId = generated!.KeyId }),
        };
        confirmRequest.Headers.TryAddWithoutValidation("X-CSRF-Token", ReadSetCookie(complete, "fd_http_csrf"));
        var confirmed = await client.SendAsync(confirmRequest);
        confirmed.EnsureSuccessStatusCode();

        var status = await client.GetAsync("/api/v1/system/status");
        Assert.True(status.IsSuccessStatusCode);
    }

    [Fact]
    public async Task SetupLockAllowsTheUiButKeepsApiLocked()
    {
        var client = _factory.CreateClient();
        Assert.NotEqual(HttpStatusCode.Locked, (await client.GetAsync("/")).StatusCode);
        Assert.NotEqual(HttpStatusCode.Locked, (await client.GetAsync("/assets/app.js")).StatusCode);
        Assert.Equal(HttpStatusCode.Locked, (await client.GetAsync("/api/v1/system/status")).StatusCode);
    }

    [Fact]
    public async Task FailedCoreSetupCanBeRetriedAndLostResponsesDoNotDuplicateAdmins()
    {
        using var factory = _factory.WithWebHostBuilder(builder => builder.ConfigureServices(services =>
        {
            services.RemoveAll<ICoreGateway>();
            services.AddSingleton<ICoreGateway, FailOnceGateway>();
        }));
        var client = factory.CreateClient();
        var issued = await client.PostAsync("/admin/setup-code", null);
        var code = (await issued.Content.ReadFromJsonAsync<CodeResponse>())!.Code;
        var body = new { code, applianceName = "harbor", adminUsername = "retry", adminPassword = "correct horse", adminPasswordConfirmation = "correct horse" };
        Assert.Equal(HttpStatusCode.ServiceUnavailable, (await client.PostAsJsonAsync("/api/v1/setup/complete", body)).StatusCode);
        using (var scope = factory.Services.CreateScope())
        {
            Assert.Empty(scope.ServiceProvider.GetRequiredService<AppDbContext>().Administrators);
        }
        var result = await client.PostAsJsonAsync("/api/v1/setup/complete", body);
        result.EnsureSuccessStatusCode();
        client.DefaultRequestHeaders.Add("X-CSRF-Token", ReadSetCookie(result, "fd_http_csrf"));
        (await client.PostAsJsonAsync("/api/v1/setup/complete", body)).EnsureSuccessStatusCode();
        using (var scope = factory.Services.CreateScope())
        {
            Assert.Single(scope.ServiceProvider.GetRequiredService<AppDbContext>().Administrators);
        }
    }

    private sealed class FailOnceGateway : ICoreGateway
    {
        private readonly LocalCoreGateway _inner = new();
        private bool _failed;
        public Task ApplyApplianceNameAsync(string name, string actor, string id, CancellationToken ct = default)
        {
            if (!_failed) { _failed = true; throw new RpcException(new Status(StatusCode.Unavailable, "down")); }
            return _inner.ApplyApplianceNameAsync(name, actor, id, ct);
        }
        public Task<GeneratedRecoveryKeyDto> GenerateRecoveryKeyAsync(string actor, string id, CancellationToken ct = default) => _inner.GenerateRecoveryKeyAsync(actor, id, ct);
        public Task ConfirmRecoveryKeyAsync(string id, string actor, string requestId, CancellationToken ct = default) => _inner.ConfirmRecoveryKeyAsync(id, actor, requestId, ct);
        public Task<SystemStatusDto> GetSystemStatusAsync(CancellationToken ct = default) => _inner.GetSystemStatusAsync(ct);
    }

    private static string ReadSetCookie(HttpResponseMessage response, string name)
    {
        if (!response.Headers.TryGetValues("Set-Cookie", out var cookies))
        {
            return "";
        }

        foreach (var header in cookies)
        {
            var trimmed = header.Trim();
            if (trimmed.StartsWith(name + "=", StringComparison.Ordinal))
            {
                return trimmed.Split(';', 2)[0][(name.Length + 1)..];
            }
        }

        return "";
    }

    private sealed record CodeResponse(string Code);
    private sealed record GeneratedResponse(string KeyId, string RecoveryKey);
}
